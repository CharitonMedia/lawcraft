//! Human-only / automation invocation guards for optional modules (LawCraft court approval).
//!
//! Kept beside WordCraft's file-access roots so MCP/agent gating stays out of that core.

use std::cell::RefCell;

use super::current;

thread_local! {
    /// Nesting depth for [`invoke_automation`]: MCP and other agent entry points.
    static AUTOMATION: RefCell<u32> = const { RefCell::new(0) };
}

/// Automation filesystem roots are in force on this thread (`confine`, `confine_process`, or the
/// process roots). Keyboard-driven work runs inside [`super::unconfined`], which clears them.
pub fn automation_confined() -> bool {
    current().is_some()
}

/// True when a command runs on behalf of automation (MCP, an agent), not the person at the keyboard.
pub fn automation_invoked() -> bool {
    AUTOMATION.with(|a| *a.borrow() > 0)
}

/// Run `f` as an automation invocation (nested calls stay automation until the outermost ends).
pub fn invoke_automation<T>(f: impl FnOnce() -> T) -> T {
    /// Lowers the automation depth again when the call ends, even when it panics.
    struct Restore;
    impl Drop for Restore {
        /// Leave the automation call this guard opened.
        fn drop(&mut self) {
            AUTOMATION.with(|a| {
                let mut n = a.borrow_mut();
                *n = n.saturating_sub(1);
            });
        }
    }
    AUTOMATION.with(|a| *a.borrow_mut() += 1);
    let _restore = Restore;
    f()
}

/// Refuse when automation invoked the command or filesystem roots confine this thread: the action is
/// for the person at the keyboard only.
pub fn require_human(what: &str) -> Result<(), String> {
    if automation_invoked() || automation_confined() {
        return Err(format!("{what} is only available outside automation (not from MCP or a confined agent session)"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_access::{AutomationRoots, confine};

    #[test]
    fn human_only_actions_are_refused_under_automation_without_roots() {
        assert!(!automation_invoked());
        assert_eq!(require_human("test"), Ok(()));
        invoke_automation(|| {
            assert!(automation_invoked());
            assert!(!automation_confined(), "filesystem roots are separate from automation");
            let e = require_human("approving a court profile").unwrap_err();
            assert!(e.contains("outside automation"), "{e}");
        });
        assert!(!automation_invoked());
    }

    #[test]
    fn human_only_actions_are_refused_when_filesystem_roots_confine_the_thread() {
        let base = std::env::temp_dir().join(format!("wordcraft-human-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("in")).expect("dir");
        let roots = AutomationRoots::new(Some(base.join("in").as_path()), None).expect("roots").expect("roots");
        assert!(!automation_invoked());
        confine(Some(&roots), || {
            assert!(automation_confined());
            assert!(!automation_invoked(), "roots alone are enough to confine agents");
            let e = require_human("approving a court profile").unwrap_err();
            assert!(e.contains("outside automation"), "{e}");
        });
        assert!(!automation_confined());
        let _ = std::fs::remove_dir_all(&base);
    }
}
