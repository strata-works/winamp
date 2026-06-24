use std::rc::Rc;
use std::time::Duration;

use crate::command::{Command, Queue, SkinSource, new_queue};
use crate::host::Host;
use crate::scene::{Pt, Scene};
use crate::script::{LoadedSkin, ScriptError};
use crate::state::StateValue;
use crate::swap::rebuild;
use crate::vocab::VocabRegistry;

pub enum PointerEvent {
    Press,
}

pub struct Engine {
    host: Box<dyn Host>,
    registry: Rc<VocabRegistry>,
    queue: Queue,
    skin: LoadedSkin,
}

impl Engine {
    pub fn new(
        host: Box<dyn Host>,
        registry: VocabRegistry,
        initial: SkinSource,
    ) -> Result<Engine, ScriptError> {
        let registry = Rc::new(registry);
        let queue = new_queue();
        let skin = rebuild(&initial, host.as_ref(), registry.clone(), queue.clone())?;
        Ok(Engine {
            host,
            registry,
            queue,
            skin,
        })
    }

    /// Phase 1 (input): resolve the hit and run the handler, which only enqueues.
    pub fn handle_pointer(&mut self, p: Pt, _kind: PointerEvent) {
        if let Some(id) = self.skin.scene.hit(p)
            && let Err(e) = self.skin.fire(id)
        {
            // A bad handler drops its command(s); the loop continues.
            eprintln!("carapace: handler error: {e:?}");
        }
    }

    /// Like `handle_pointer`, but hit-tests the layout-resolved scene for the given logical window
    /// size, so anchored hotspots (frame skins) are hit where they are actually drawn rather than
    /// at their design positions. `p` is in logical (window-point) coordinates.
    pub fn handle_pointer_resolved(
        &mut self,
        logical_w: f32,
        logical_h: f32,
        p: Pt,
        _kind: PointerEvent,
    ) {
        let hit = self.layout(logical_w, logical_h).hit(p);
        if let Some(id) = hit
            && let Err(e) = self.skin.fire(id)
        {
            eprintln!("carapace: handler error: {e:?}");
        }
    }

    /// Enqueue a meta command (the host app's Tab/H equivalents).
    pub fn handle_command(&mut self, cmd: Command) {
        self.queue.borrow_mut().push(cmd);
    }

    /// Phase 2 (drain) + Phase 3 (tick).
    pub fn update(&mut self, dt: Duration) {
        let cmds: Vec<Command> = std::mem::take(&mut *self.queue.borrow_mut());
        for cmd in cmds {
            match cmd {
                Command::HostAction { action, args } => {
                    // Validate against the CURRENT host's allowlist (handles post-switch).
                    if self.host.actions().iter().any(|a| a.name == action) {
                        self.host.invoke(&action, &args);
                    } else {
                        eprintln!("carapace: dropped action '{action}' not in host allowlist");
                    }
                }
                Command::Swap(source) => self.apply_swap(&source),
                Command::SwitchHost { host, skin } => {
                    self.host = host;
                    self.apply_swap(&skin);
                }
            }
        }
        self.host.tick(dt);
    }

    fn apply_swap(&mut self, source: &SkinSource) {
        match rebuild(
            source,
            self.host.as_ref(),
            self.registry.clone(),
            self.queue.clone(),
        ) {
            Ok(skin) => self.skin = skin, // transactional: only replace on success
            Err(e) => eprintln!("carapace: swap failed, keeping current skin: {e:?}"),
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.skin.scene
    }

    /// The per-node anchors parallel to `scene().nodes`, for the layout pass.
    pub fn scene_anchors(&self) -> &[crate::layout::Anchors] {
        &self.skin.anchors
    }

    /// Resolve the design scene to a logical scene for the given window logical size, using the
    /// skin's per-element anchors. The result's `canvas` equals the logical size, so the renderer
    /// applies only the DPI scale. Frame skins call this on resize; gadget skins render the design
    /// scene directly.
    pub fn layout(&self, logical_w: f32, logical_h: f32) -> Scene {
        crate::layout::resolve_scene(&self.skin.scene, &self.skin.anchors, (logical_w, logical_h))
    }

    pub fn state(&self, key: &str) -> Option<StateValue> {
        self.host.get(key)
    }
}
