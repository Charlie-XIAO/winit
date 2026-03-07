use std::collections::HashMap;
use std::os::fd::FromRawFd;
use std::os::unix::io::{OwnedFd, RawFd};
use std::time::Duration;
use std::{fmt, io};

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use glib::translate::ToGlibPtr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileId {
    dev: libc::dev_t,
    ino: libc::ino_t,
}

impl TryFrom<RawFd> for FileId {
    type Error = io::Error;

    fn try_from(fd: RawFd) -> Result<Self, Self::Error> {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        Ok(Self { dev: stat.st_dev, ino: stat.st_ino })
    }
}

/// Bridge a thread-default glib main context into a calloop-driven event loop.
pub struct GlibBridge<State> {
    ctx: glib::MainContext,
    regs: HashMap<RawFd, (RegistrationToken, i16, FileId)>,
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

fn interest_from_events(events: i16) -> Interest {
    let mut interest = Interest::EMPTY;

    if (events & (libc::POLLIN | libc::POLLPRI)) != 0 {
        interest.readable = true;
    }
    if (events & libc::POLLOUT) != 0 {
        interest.writable = true;
    }

    // Treat error and hangup conditions as readable, so we can wake and
    // let glib handle them
    if (events & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 {
        interest.readable = true;
    }

    // Defensive: if glib every returns an empty mask, still register
    // something so the source is not "dead" in calloop
    if !interest.readable && !interest.writable {
        interest.readable = true;
    }

    interest
}

impl<State> GlibBridge<State> {
    /// Refresh glib's requested poll set and register them into calloop.
    ///
    /// This should be called before blocking in the host event loop. Note that
    /// this does **not** run glib callbacks; [`Self::drain`] must be called to
    /// progress glib work.
    pub fn refresh(&mut self, handle: &LoopHandle<'_, State>) -> io::Result<()> {
        let _guard = self.ctx.acquire().map_err(|e| {
            self.timeout_ms = -1;
            self.ready_now = false;

            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to acquire ownership of glib main context: {e}"),
            )
        })?;

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
            return Err(io::Error::new(io::ErrorKind::Other, "g_main_context_query failed (2)"));
        }

        // Snapshot glib's currently-requested poll set
        let mut keep = HashMap::new();
        for p in self.pollfds.iter().take(got as _) {
            if p.fd >= 0 {
                let fd = p.fd as RawFd;
                let id: FileId = fd.try_into()?;
                keep.insert(fd, (p.events as i16, id));
            }
        }

        // Remove calloop sources for fds that glib no longer needs
        let to_remove: Vec<_> =
            self.regs.keys().copied().filter(|fd| !keep.contains_key(fd)).collect();
        for fd in to_remove {
            if let Some((token, ..)) = self.regs.remove(&fd) {
                handle.remove(token);
            }
        }

        for (&fd, &(events, id)) in &keep {
            // Reinstall if fd is new, or events changed, or file identity
            // changed (e.g., due to close and fd reuse)
            let needs_reinstall = match self.regs.get(&fd) {
                None => true,
                Some((_, old_events, old_id)) => *old_events != events || *old_id != id,
            };
            if !needs_reinstall {
                continue;
            }

            if let Some((old_token, ..)) = self.regs.remove(&fd) {
                handle.remove(old_token);
            }

            let interest = interest_from_events(events);

            // We dup() the fd so the calloop generic source owns a stable file
            // descriptor even if glib closes/replaces its original fd later
            let duped_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
            if duped_fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let owned_fd = unsafe { OwnedFd::from_raw_fd(duped_fd) };
            let source = Generic::new(owned_fd, interest, Mode::Level);
            let token = handle
                .insert_source(source, |_, _, _| {
                    // This source only needs to wake up the calloop event loop
                    // and does not directly dispatch glib work; glib work is
                    // separately progressed via drain()
                    Ok(PostAction::Continue)
                })
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to insert glib event source: {e}"),
                    )
                })?;
            self.regs.insert(fd, (token, events, id));
            tracing::debug!(fd, events, ?id, "register source");
        }

        Ok(())
    }

    /// Return glib's requested timeout.
    pub fn timeout(&self) -> Option<Duration> {
        if self.timeout_ms < 0 {
            None
        } else {
            Some(Duration::from_millis(self.timeout_ms as _))
        }
    }

    /// Whether glib reported immediate work ready in the last [`refresh`].
    ///
    /// [`refresh`]: Self::refresh
    pub fn ready_now(&self) -> bool {
        self.ready_now
    }

    /// Progress glib work non-blockingly until no further work is ready.
    pub fn drain(&mut self) {
        while self.ctx.iteration(false) {}
    }
}

impl<State> fmt::Debug for GlibBridge<State> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct PollFdList<'a>(&'a [glib::ffi::GPollFD]);

        impl fmt::Debug for PollFdList<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list().entries(self.0.iter().map(|p| (p.fd, p.events, p.revents))).finish()
            }
        }

        f.debug_struct("GlibBridge")
            .field("ctx", &self.ctx)
            .field("regs", &self.regs)
            .field("pollfds", &PollFdList(&self.pollfds))
            .field("timeout_ms", &self.timeout_ms)
            .field("ready_now", &self.ready_now)
            .finish()
    }
}
