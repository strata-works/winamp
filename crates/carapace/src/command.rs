use std::cell::RefCell;
use std::rc::Rc;

use crate::host::{Host, Value};

#[derive(Clone, Debug)]
pub struct SkinSource {
    pub lua_src: String,
    pub canvas: (u32, u32),
}

pub enum Command {
    HostAction {
        action: String,
        args: Vec<Value>,
    },
    Swap(SkinSource),
    SwitchHost {
        host: Box<dyn Host>,
        skin: SkinSource,
    },
}

/// Shared command queue: skin handlers push HostAction; the host app pushes
/// Swap/SwitchHost; the Engine drains it.
pub type Queue = Rc<RefCell<Vec<Command>>>;

pub fn new_queue() -> Queue {
    Rc::new(RefCell::new(Vec::new()))
}
