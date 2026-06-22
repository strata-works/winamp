use carapace::host::ActionSpec;

#[derive(Debug, PartialEq)]
pub enum WindowOp {
    BeginDrag,
    Minimize,
    Close,
}

pub type WindowOutbox = std::rc::Rc<std::cell::RefCell<Vec<WindowOp>>>;

pub const WINDOW_ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "begin_drag" },
    ActionSpec { name: "minimize" },
    ActionSpec { name: "close" },
];

/// Records the matching window op; returns true iff `action` was a window-control action.
pub fn handle_window_action(action: &str, out: &WindowOutbox) -> bool {
    let op = match action {
        "begin_drag" => WindowOp::BeginDrag,
        "minimize" => WindowOp::Minimize,
        "close" => WindowOp::Close,
        _ => return false,
    };
    out.borrow_mut().push(op);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_window_ops_and_ignores_others() {
        let out: WindowOutbox = Default::default();
        assert!(handle_window_action("minimize", &out));
        assert!(handle_window_action("close", &out));
        assert!(
            !handle_window_action("toggle_play", &out),
            "domain action is not window-control"
        );
        assert_eq!(&*out.borrow(), &[WindowOp::Minimize, WindowOp::Close]);
    }
}
