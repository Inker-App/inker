//! Who is connected right now, for the popover's "Claude connected".
//!
//! Each served connection registers here, names itself once the client's
//! `initialize` arrives, and leaves when the connection ends. Only product
//! names are kept (see `display_agent_name`), never anything a client sent
//! beyond its name.

use std::sync::{Arc, Mutex, MutexGuard};

type OnChange = Box<dyn Fn() + Send + Sync>;

/// Live sessions, shared by every connection the host serves.
#[derive(Default)]
pub struct Sessions {
    inner: Mutex<Inner>,
    on_change: Mutex<Option<OnChange>>,
}

#[derive(Default)]
struct Inner {
    next: u64,
    open: Vec<(u64, Option<String>)>,
}

impl std::fmt::Debug for Sessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessions")
            .field("clients", &self.clients())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Sessions {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Called whenever a client arrives or leaves (after the change).
    pub fn set_on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        *lock(&self.on_change) = Some(Box::new(f));
    }

    /// A connection opened; it has no name until `initialize`.
    pub fn open(&self) -> u64 {
        let mut inner = lock(&self.inner);
        let id = inner.next;
        inner.next += 1;
        inner.open.push((id, None));
        id
    }

    /// The client introduced itself.
    pub fn name(&self, id: u64, display_name: String) {
        {
            let mut inner = lock(&self.inner);
            if let Some(slot) = inner.open.iter_mut().find(|(i, _)| *i == id) {
                slot.1 = Some(display_name);
            }
        }
        self.changed();
    }

    /// The connection ended.
    pub fn close(&self, id: u64) {
        let named = {
            let mut inner = lock(&self.inner);
            let before = inner.open.len();
            let was_named = inner
                .open
                .iter()
                .any(|(i, name)| *i == id && name.is_some());
            inner.open.retain(|(i, _)| *i != id);
            was_named && inner.open.len() < before
        };
        if named {
            self.changed();
        }
    }

    /// The product names of the clients that have initialized, each once,
    /// sorted.
    pub fn clients(&self) -> Vec<String> {
        let mut names: Vec<String> = lock(&self.inner)
            .open
            .iter()
            .filter_map(|(_, n)| n.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn changed(&self) {
        if let Some(f) = lock(&self.on_change).as_ref() {
            f();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn a_session_counts_once_it_has_a_name_and_leaves_on_close() {
        let s = Sessions::new();
        let a = s.open();
        assert!(s.clients().is_empty(), "not initialized yet");
        s.name(a, "Claude".into());
        assert_eq!(s.clients(), vec!["Claude"]);
        let b = s.open();
        s.name(b, "Claude".into());
        let c = s.open();
        s.name(c, "Claude Code".into());
        assert_eq!(
            s.clients(),
            vec!["Claude", "Claude Code"],
            "each name once, sorted"
        );
        s.close(a);
        assert_eq!(
            s.clients(),
            vec!["Claude", "Claude Code"],
            "the second Claude window is still here"
        );
        s.close(b);
        s.close(c);
        assert!(s.clients().is_empty());
    }

    #[test]
    fn arrivals_and_departures_fire_the_callback_but_an_unnamed_peer_does_not() {
        let s = Sessions::new();
        let fired = Arc::new(AtomicUsize::new(0));
        let f = fired.clone();
        s.set_on_change(move || {
            f.fetch_add(1, Ordering::SeqCst);
        });
        let ghost = s.open();
        s.close(ghost);
        assert_eq!(
            fired.load(Ordering::SeqCst),
            0,
            "connected and left without initializing"
        );
        let a = s.open();
        s.name(a, "Claude".into());
        s.close(a);
        assert_eq!(fired.load(Ordering::SeqCst), 2);
    }
}
