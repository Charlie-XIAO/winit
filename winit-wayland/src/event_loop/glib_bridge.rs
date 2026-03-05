use std::collections::HashMap;
use std::os::fd::FromRawFd;
use std::os::unix::io::{OwnedFd, RawFd};
use std::time::Duration;
use std::{fmt, io};

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use glib::translate::ToGlibPtr;

pub struct GlibBridge<State> {
    ctx: glib::MainContext,
    regs: HashMap<RawFd, (RegistrationToken, i16)>,
    pollfds: Vec<glib::ffi::GPollFD>,
    timeout_ms: i32,
    ready_now: bool,
    _phantom: std::marker::PhantomData<State>,
}

impl<State> Default for GlibBridge<State> {
    fn default() -> Self {
        Self {
            ctx: glib::MainContext::ref_thread_default(),
            regs: HashMap::new(),
            pollfds: Vec::new(),
            timeout_ms: -1,
            ready_now: false,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<State> GlibBridge<State> {
    pub fn refresh(&mut self, handle: &LoopHandle<'_, State>) -> io::Result<()> {
        let _guard = match self.ctx.acquire() {
            Ok(guard) => guard,
            Err(e) => {
                self.timeout_ms = -1;
                self.ready_now = false;
                tracing::warn!("Failed to acquire ownership of glib main context: {e}");
                return Ok(());
            },
        };

        let (ready_now, priority) = self.ctx.prepare();
        self.ready_now = ready_now;

        self.timeout_ms = -1;
        let needed = unsafe {
            glib::ffi::g_main_context_query(
                self.ctx.to_glib_none().0,
                priority,
                &mut self.timeout_ms,
                std::ptr::null_mut(),
                0,
            )
        };
        if needed < 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "g_main_context_query failed"));
        }

        self.pollfds.resize_with(needed as _, || glib::ffi::GPollFD {
            fd: -1,
            events: 0,
            revents: 0,
        });

        let got = unsafe {
            glib::ffi::g_main_context_query(
                self.ctx.to_glib_none().0,
                priority,
                &mut self.timeout_ms,
                self.pollfds.as_mut_ptr(),
                self.pollfds.len() as _,
            )
        };
        if got < 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "g_main_context_query failed"));
        }

        let mut keep: HashMap<RawFd, i16> = HashMap::new();
        for p in &self.pollfds {
            if p.fd >= 0 {
                keep.insert(p.fd as _, p.events as _);
            }
        }

        let to_remove: Vec<_> =
            self.regs.keys().copied().filter(|fd| !keep.contains_key(fd)).collect();
        for fd in to_remove {
            if let Some((token, _)) = self.regs.remove(&fd) {
                let _ = handle.remove(token);
            }
        }

        for (&fd, &events) in &keep {
            let needs_reinstall = match self.regs.get(&fd) {
                None => true,
                Some((_, old_events)) => *old_events != events,
            };
            if !needs_reinstall {
                continue;
            }

            let mut interest = Interest::EMPTY;
            if events & libc::POLLIN != 0 {
                interest.readable = true;
            }
            if events & libc::POLLOUT != 0 {
                interest.writable = true;
            }
            if events & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                interest.readable = true;
            }
            if !interest.readable && !interest.writable {
                interest.readable = true; // Avoid dead source
            }

            let duped_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
            if duped_fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let owned_fd = unsafe { OwnedFd::from_raw_fd(duped_fd) };
            let source = Generic::new(owned_fd, interest, Mode::Level);
            let token = handle
                .insert_source(source, move |_, _, _| {
                    Ok(PostAction::Continue) // No direct action, wakup is enough
                })
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to insert glib event source: {e}"),
                    )
                })?;
            self.regs.insert(fd, (token, events));
        }

        Ok(())
    }

    pub fn timeout(&self) -> Option<Duration> {
        if self.timeout_ms < 0 { None } else { Some(Duration::from_millis(self.timeout_ms as _)) }
    }

    pub fn ready_now(&self) -> bool {
        self.ready_now
    }

    pub fn drain(&mut self) {
        loop {
            let dispatched = self.ctx.iteration(false);
            if !dispatched {
                break;
            }
        }
    }
}

impl<State> fmt::Debug for GlibBridge<State> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pollfds = f
            .debug_list()
            .entries(self.pollfds.iter().map(|p| (p.fd, p.events, p.revents)))
            .finish();

        f.debug_struct("GlibBridge")
            .field("ctx", &self.ctx)
            .field("regs", &self.regs)
            .field("pollfds", &pollfds)
            .field("timeout_ms", &self.timeout_ms)
            .field("ready_now", &self.ready_now)
            .finish()
    }
}
