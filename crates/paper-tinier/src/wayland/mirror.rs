use super::{BUFFER_COUNT, Wallpaper, resolve_viewport};
use paper_runtime::mirror::{Layout, Link, Memory, Message, Plane, XR24};
use std::os::fd::{AsFd, RawFd};
use std::time::{Duration, Instant};

const HOLD_LIMIT: Duration = Duration::from_millis(250);
pub(super) const HOLD_POLL_MS: i32 = 50;

pub(super) struct Mirror {
    link: Link,
    sent: [bool; BUFFER_COUNT],
    held: [Option<Instant>; BUFFER_COUNT],
}

impl Mirror {
    pub(super) fn fd(&self) -> RawFd {
        self.link.fd()
    }
}

impl Wallpaper {
    pub(super) fn mirror_held(&mut self, index: usize) -> bool {
        for surface in &mut self.state.surfaces {
            if surface.mirror.as_ref().is_some_and(|mirror| {
                mirror.held[index].is_some_and(|since| since.elapsed() >= HOLD_LIMIT)
            }) {
                surface.mirror = None;
            }
        }
        self.state.surfaces.iter().any(|surface| {
            surface.mirror.as_ref().is_some_and(|mirror| mirror.held[index].is_some())
        })
    }

    pub(super) fn service_mirrors(&mut self) {
        for target in crate::MIRRORS.lock().unwrap().drain(..) {
            let Some(surface) = self
                .state
                .surfaces
                .iter_mut()
                .find(|surface| surface.name == target.output && !surface.closed)
            else {
                continue;
            };
            surface.mirror = None;
            if target.socket.is_empty() {
                continue;
            }
            match Link::connect(std::path::Path::new(&target.socket)) {
                Ok(link) => {
                    surface.mirror = Some(Mirror {
                        link,
                        sent: [false; BUFFER_COUNT],
                        held: [None; BUFFER_COUNT],
                    });
                }
                Err(error) => eprintln!("skwd-paper-tinier: mirror connect failed: {error}"),
            }
            if let Some(index) = self.shown {
                self.publish_mirrors(index);
            }
        }
        for surface in &mut self.state.surfaces {
            let Some(mirror) = surface.mirror.as_mut() else { continue };
            match mirror.link.drain() {
                Ok(released) => {
                    for id in released {
                        if let Some(held) = mirror.held.get_mut(usize::from(id)) {
                            *held = None;
                        }
                    }
                }
                Err(_) => surface.mirror = None,
            }
        }
    }

    pub(super) fn publish_mirrors(&mut self, index: usize) {
        for surface in &mut self.state.surfaces {
            let Some(mirror) = surface.mirror.as_mut() else { continue };
            let id = index as u8;
            let (_, _, (x, y, width, height)) = resolve_viewport(
                self.video,
                surface.output_width as u32,
                surface.output_height as u32,
            );
            let layout = Layout {
                width: self.video.width,
                height: self.video.height,
                fourcc: XR24,
                modifier: 0,
                memory: Memory::Shm,
                planes: [
                    Plane {
                        stride: self.video.stride,
                        offset: (index * self.video.frame_bytes) as u32,
                    },
                    Plane::default(),
                ],
                plane_count: 1,
            };
            let result = if mirror.sent[index] {
                Ok(())
            } else {
                mirror.link.send(&Message::Buffer { id, layout }, &[self.buffers.memory.as_fd()])
            }
            .and_then(|()| {
                mirror.link.send(&Message::Show { id, source: [x, y, width, height] }, &[])
            });
            if result.is_ok() {
                mirror.sent[index] = true;
                mirror.held[index] = Some(Instant::now());
            } else {
                surface.mirror = None;
            }
        }
    }
}
