//! Terminal lifetime is guarded across startup errors and panic unwinding.
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Write};

pub trait TerminalOps {
    fn raw(&mut self) -> io::Result<()>;
    fn screen(&mut self) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}

pub struct CrosstermOps;
impl TerminalOps for CrosstermOps {
    fn raw(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }
    fn screen(&mut self) -> io::Result<()> {
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            Hide
        )
    }
    fn restore(&mut self) -> io::Result<()> {
        let screen = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            Show,
            LeaveAlternateScreen
        );
        let raw = disable_raw_mode();
        let flush = io::stdout().flush();
        screen.and(raw).and(flush)
    }
}

#[must_use = "keep the guard alive for the terminal session"]
pub struct TerminalGuard<T: TerminalOps = CrosstermOps> {
    ops: T,
}

impl TerminalGuard<CrosstermOps> {
    pub fn enter() -> io::Result<Self> {
        Self::initialize(CrosstermOps)
    }
}

impl<T: TerminalOps> TerminalGuard<T> {
    fn initialize(ops: T) -> io::Result<Self> {
        let mut guard = Self { ops };
        guard.ops.raw()?;
        guard.ops.screen()?;
        Ok(guard)
    }

    pub fn suspend(&mut self) -> io::Result<()> {
        self.ops.restore()
    }

    pub fn resume(&mut self) -> io::Result<()> {
        let result = self.ops.raw().and_then(|()| self.ops.screen());
        if result.is_err() {
            let _ = self.ops.restore();
        }
        result
    }
}

impl<T: TerminalOps> Drop for TerminalGuard<T> {
    fn drop(&mut self) {
        let _ = self.ops.restore();
    }
}

pub fn restore_terminal() {
    let _ = CrosstermOps.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    struct Fake {
        events: Rc<RefCell<Vec<&'static str>>>,
        fail: Option<&'static str>,
    }
    impl TerminalOps for Fake {
        fn raw(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push("raw");
            if self.fail == Some("raw") {
                Err(io::Error::other("raw failure"))
            } else {
                Ok(())
            }
        }
        fn screen(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push("screen");
            if self.fail == Some("screen") {
                Err(io::Error::other("screen failure"))
            } else {
                Ok(())
            }
        }
        fn restore(&mut self) -> io::Result<()> {
            self.events
                .borrow_mut()
                .push("restore cursor/mouse/alternate/raw");
            Ok(())
        }
    }

    #[test]
    fn bootstrap_injected_initialization_failure_restores_every_lifetime_phase() {
        for failed_phase in ["raw", "screen"] {
            let events = Rc::new(RefCell::new(Vec::new()));
            assert!(TerminalGuard::initialize(Fake {
                events: events.clone(),
                fail: Some(failed_phase)
            })
            .is_err());
            let expected = if failed_phase == "raw" {
                vec!["raw", "restore cursor/mouse/alternate/raw"]
            } else {
                vec!["raw", "screen", "restore cursor/mouse/alternate/raw"]
            };
            assert_eq!(*events.borrow(), expected);
        }
    }

    #[test]
    fn bootstrap_panic_unwinding_restores_guard_once() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let _guard = TerminalGuard::initialize(Fake {
                    events: observed,
                    fail: None,
                })
                .unwrap();
                panic!("injected app panic");
            }))
            .is_err()
        );
        assert_eq!(
            *events.borrow(),
            ["raw", "screen", "restore cursor/mouse/alternate/raw"]
        );
    }

    #[test]
    fn composer_editor_handoff_restores_and_resumes_terminal_lifetime() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut guard = TerminalGuard::initialize(Fake {
            events: events.clone(),
            fail: None,
        })
        .unwrap();
        guard.suspend().unwrap();
        guard.resume().unwrap();
        drop(guard);
        assert_eq!(
            *events.borrow(),
            [
                "raw",
                "screen",
                "restore cursor/mouse/alternate/raw",
                "raw",
                "screen",
                "restore cursor/mouse/alternate/raw"
            ]
        );
    }
}
