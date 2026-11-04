//! Who is connected, and how to reach them.
//!
//! `SupportHub` is the boundary the rest of the chat code talks to. `LocalHub`
//! keeps everything in this process, which is right for a single server
//! instance. To run several instances, implement the trait on top of Redis
//! pub/sub or Postgres LISTEN/NOTIFY: `publish` fans a frame out to the
//! instance-local connections of the audience *and* to the other instances,
//! and `register`/`unregister` track presence across them. Nothing outside
//! this module needs to change.

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use dashmap::{DashMap, DashSet};
use tokio::sync::{Notify, mpsc};
use uuid::Uuid;

use crate::support::protocol::ChatRole;

pub type ConnId = u64;

/// Who should receive a frame.
#[derive(Debug, Clone, Copy)]
pub enum Audience {
    /// Every connection (tab) of one user.
    User(Uuid),
    /// Every admin connection.
    Admins,
    /// Admin connections that joined this conversation.
    AdminsInConversation(Uuid),
    /// One connection.
    Connection(ConnId),
}

#[derive(Debug, Clone, Copy)]
pub struct HubConfig {
    /// Frames buffered per connection before it counts as a slow consumer.
    pub queue_capacity: usize,
    pub max_user_connections: usize,
    pub max_admin_connections: usize,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 256,
            max_user_connections: 5,
            max_admin_connections: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HubError {
    #[error("too many open connections for this account")]
    TooManyConnections,
}

pub struct Registration {
    pub conn_id: ConnId,
    /// Frames for this connection. Closed when the hub drops the connection.
    pub rx: mpsc::Receiver<Arc<str>>,
    /// Notified when the hub gave up on this connection (queue full): close it.
    pub kick: Arc<Notify>,
    /// This was the user's first live connection.
    pub became_online: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Unregistered {
    pub user_id: Uuid,
    pub role: ChatRole,
    /// That was the user's last live connection.
    pub went_offline: bool,
}

pub trait SupportHub: Send + Sync {
    fn register(&self, user_id: Uuid, role: ChatRole) -> Result<Registration, HubError>;
    /// Idempotent: `None` when the connection was already gone.
    fn unregister(&self, conn: ConnId) -> Option<Unregistered>;
    /// Never blocks: a connection whose queue is full is kicked, not waited for.
    fn publish(&self, audience: Audience, frame: Arc<str>);
    fn join(&self, conn: ConnId, conversation: Uuid);
    fn leave(&self, conn: ConnId, conversation: Uuid);
    /// Which of these users have at least one live connection.
    fn online_users(&self, ids: &[Uuid]) -> HashSet<Uuid>;
    fn connection_count(&self) -> usize;
}

pub type DynSupportHub = Arc<dyn SupportHub>;

struct Conn {
    user_id: Uuid,
    role: ChatRole,
    tx: mpsc::Sender<Arc<str>>,
    kick: Arc<Notify>,
    joined: Mutex<HashSet<Uuid>>,
}

pub struct LocalHub {
    cfg: HubConfig,
    next_id: AtomicU64,
    conns: DashMap<ConnId, Conn>,
    by_user: DashMap<Uuid, Vec<ConnId>>,
    admins: DashSet<ConnId>,
}

impl LocalHub {
    pub fn new(cfg: HubConfig) -> Self {
        Self {
            cfg,
            next_id: AtomicU64::new(1),
            conns: DashMap::new(),
            by_user: DashMap::new(),
            admins: DashSet::new(),
        }
    }

    fn deliver(&self, conn_id: ConnId, frame: &Arc<str>) {
        let Some(conn) = self.conns.get(&conn_id) else {
            return;
        };
        match conn.tx.try_send(frame.clone()) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(conn_id, "Chat connection is too slow; closing it");
                conn.kick.notify_one();
            }
            // The connection task is already shutting down.
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }
}

impl SupportHub for LocalHub {
    fn register(&self, user_id: Uuid, role: ChatRole) -> Result<Registration, HubError> {
        let limit = match role {
            ChatRole::User => self.cfg.max_user_connections,
            ChatRole::Admin => self.cfg.max_admin_connections,
        };
        let conn_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(self.cfg.queue_capacity);
        let kick = Arc::new(Notify::new());

        // The entry guard makes "count, then add" atomic for this user.
        let mut ids = self.by_user.entry(user_id).or_default();
        if ids.len() >= limit {
            return Err(HubError::TooManyConnections);
        }
        let became_online = ids.is_empty();
        self.conns.insert(
            conn_id,
            Conn {
                user_id,
                role,
                tx,
                kick: kick.clone(),
                joined: Mutex::new(HashSet::new()),
            },
        );
        ids.push(conn_id);
        drop(ids);
        if role == ChatRole::Admin {
            self.admins.insert(conn_id);
        }
        Ok(Registration {
            conn_id,
            rx,
            kick,
            became_online,
        })
    }

    fn unregister(&self, conn_id: ConnId) -> Option<Unregistered> {
        let (_, conn) = self.conns.remove(&conn_id)?;
        self.admins.remove(&conn_id);
        let mut went_offline = false;
        if let dashmap::mapref::entry::Entry::Occupied(mut entry) = self.by_user.entry(conn.user_id)
        {
            entry.get_mut().retain(|id| *id != conn_id);
            if entry.get().is_empty() {
                entry.remove();
                went_offline = true;
            }
        }
        Some(Unregistered {
            user_id: conn.user_id,
            role: conn.role,
            went_offline,
        })
    }

    fn publish(&self, audience: Audience, frame: Arc<str>) {
        match audience {
            Audience::Connection(id) => self.deliver(id, &frame),
            Audience::User(user_id) => {
                if let Some(ids) = self.by_user.get(&user_id) {
                    for id in ids.iter() {
                        self.deliver(*id, &frame);
                    }
                }
            }
            Audience::Admins => {
                for id in self.admins.iter() {
                    self.deliver(*id, &frame);
                }
            }
            Audience::AdminsInConversation(conversation) => {
                for id in self.admins.iter() {
                    let joined = self
                        .conns
                        .get(&*id)
                        .is_some_and(|c| c.joined.lock().is_ok_and(|j| j.contains(&conversation)));
                    if joined {
                        self.deliver(*id, &frame);
                    }
                }
            }
        }
    }

    fn join(&self, conn: ConnId, conversation: Uuid) {
        if let Some(c) = self.conns.get(&conn)
            && let Ok(mut joined) = c.joined.lock()
        {
            joined.insert(conversation);
        }
    }

    fn leave(&self, conn: ConnId, conversation: Uuid) {
        if let Some(c) = self.conns.get(&conn)
            && let Ok(mut joined) = c.joined.lock()
        {
            joined.remove(&conversation);
        }
    }

    fn online_users(&self, ids: &[Uuid]) -> HashSet<Uuid> {
        ids.iter()
            .filter(|id| self.by_user.contains_key(id))
            .copied()
            .collect()
    }

    fn connection_count(&self) -> usize {
        self.conns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_limits_and_cleanup() {
        let hub = LocalHub::new(HubConfig {
            queue_capacity: 2,
            max_user_connections: 2,
            max_admin_connections: 1,
        });
        let u = Uuid::new_v4();
        let a = hub.register(u, ChatRole::User).unwrap();
        assert!(a.became_online);
        let b = hub.register(u, ChatRole::User).unwrap();
        assert!(!b.became_online);
        assert_eq!(
            hub.register(u, ChatRole::User).err(),
            Some(HubError::TooManyConnections)
        );
        assert!(hub.online_users(&[u]).contains(&u));

        assert!(!hub.unregister(a.conn_id).unwrap().went_offline);
        assert!(hub.unregister(b.conn_id).unwrap().went_offline);
        assert!(hub.unregister(b.conn_id).is_none());
        assert_eq!(hub.connection_count(), 0);
        assert!(hub.by_user.is_empty());
    }

    #[tokio::test]
    async fn slow_consumer_is_kicked() {
        let hub = LocalHub::new(HubConfig {
            queue_capacity: 1,
            ..HubConfig::default()
        });
        let u = Uuid::new_v4();
        let reg = hub.register(u, ChatRole::User).unwrap();
        hub.publish(Audience::User(u), Arc::from("a"));
        hub.publish(Audience::User(u), Arc::from("b"));
        tokio::time::timeout(std::time::Duration::from_secs(1), reg.kick.notified())
            .await
            .expect("kicked");
    }

    #[tokio::test]
    async fn audiences_are_scoped() {
        let hub = LocalHub::new(HubConfig::default());
        let (u1, u2, adm) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let conv = Uuid::new_v4();
        let mut r1 = hub.register(u1, ChatRole::User).unwrap();
        let mut r2 = hub.register(u2, ChatRole::User).unwrap();
        let mut ra = hub.register(adm, ChatRole::Admin).unwrap();

        hub.publish(Audience::User(u1), Arc::from("to-u1"));
        hub.publish(Audience::AdminsInConversation(conv), Arc::from("joined"));
        hub.join(ra.conn_id, conv);
        hub.publish(Audience::AdminsInConversation(conv), Arc::from("joined2"));
        hub.publish(Audience::Admins, Arc::from("all-admins"));

        assert_eq!(&*r1.rx.try_recv().unwrap(), "to-u1");
        assert!(r1.rx.try_recv().is_err());
        assert!(r2.rx.try_recv().is_err());
        assert_eq!(&*ra.rx.try_recv().unwrap(), "joined2");
        assert_eq!(&*ra.rx.try_recv().unwrap(), "all-admins");
    }
}
