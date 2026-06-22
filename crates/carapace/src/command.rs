use std::cell::RefCell;
use std::rc::Rc;

use crate::asset::AssetResolver;
use crate::host::{Host, Value};

#[derive(Clone)]
pub struct SkinSource {
    pub lua_src: String,
    pub canvas: (u32, u32),
    pub assets: Rc<AssetResolver>,
}

impl SkinSource {
    /// An inline skin with no assets (tests, asset-free skins).
    pub fn inline(lua_src: impl Into<String>, canvas: (u32, u32)) -> Self {
        Self {
            lua_src: lua_src.into(),
            canvas,
            assets: Rc::new(AssetResolver::empty()),
        }
    }
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

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::HostAction { action, args } => f
                .debug_struct("HostAction")
                .field("action", action)
                .field("args", args)
                .finish(),
            Command::Swap(_) => f.debug_tuple("Swap").field(&"<SkinSource>").finish(),
            Command::SwitchHost { .. } => f.write_str("SwitchHost { .. }"),
        }
    }
}

/// Shared command queue: skin handlers push HostAction; the host app pushes
/// Swap/SwitchHost; the Engine drains it.
pub type Queue = Rc<RefCell<Vec<Command>>>;

pub fn new_queue() -> Queue {
    Rc::new(RefCell::new(Vec::new()))
}
