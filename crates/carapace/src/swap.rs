use std::rc::Rc;

use crate::command::{Queue, SkinSource};
use crate::host::Host;
use crate::script::{load, LoadedSkin, ScriptError};
use crate::vocab::VocabRegistry;

/// Build a fresh skin from `source`. On error the caller keeps its current skin
/// (transactional swap — the rebuild never mutates the caller's state).
pub fn rebuild(
    source: &SkinSource,
    host: &dyn Host,
    registry: Rc<VocabRegistry>,
    queue: Queue,
) -> Result<LoadedSkin, ScriptError> {
    load(source, host, registry, queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::new_queue;
    use crate::fixture::FixtureHost;

    fn src(s: &str) -> SkinSource {
        SkinSource { lua_src: s.to_string(), canvas: (10, 10) }
    }

    #[test]
    fn rebuild_ok_returns_new_scene() {
        let skin = rebuild(
            &src("fill{ path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color={r=0,g=0,b=0} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            new_queue(),
        )
        .unwrap();
        assert_eq!(skin.scene.nodes.len(), 1);
    }

    #[test]
    fn rebuild_err_on_bad_skin() {
        let r = rebuild(
            &src("this is not lua {{{"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            new_queue(),
        );
        assert!(r.is_err());
    }
}
