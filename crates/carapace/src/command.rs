use std::cell::RefCell;
use std::rc::Rc;

use crate::asset::AssetResolver;
use crate::host::{Host, Value};

/// The engine-facing payload of a skin: its Lua source, design canvas size,
/// and resolved assets. Built by `skin::load_dir` or [`SkinSource::inline`]
/// and passed to `Engine::new` (or carried by the `Swap`/`SwitchHost` commands).
#[derive(Clone)]
pub struct SkinSource {
    /// The skin's Lua entry source.
    pub lua_src: String,
    /// The design-resolution `(width, height)` the skin authors against.
    pub canvas: (u32, u32),
    /// Resolved asset lookup for this skin (images, fonts, etc).
    pub assets: Rc<AssetResolver>,
}

impl SkinSource {
    /// Builds a source with no on-disk assets, for tests or asset-free skins.
    pub fn inline(lua_src: impl Into<String>, canvas: (u32, u32)) -> Self {
        Self {
            lua_src: lua_src.into(),
            canvas,
            assets: Rc::new(AssetResolver::empty()),
        }
    }
}

/// A queued action for the engine to apply on the next `Engine::update`.
pub enum Command {
    /// Invoke a host action (validated against the host's allowlist before dispatch).
    HostAction {
        /// The action name.
        action: String,
        /// Arguments to pass to `Host::invoke`.
        args: Vec<Value>,
    },
    /// Replace the current skin, keeping the current host.
    Swap(SkinSource),
    /// Replace both the current host and the current skin.
    SwitchHost {
        /// The new host.
        host: Box<dyn Host>,
        /// The skin to load with the new host.
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
