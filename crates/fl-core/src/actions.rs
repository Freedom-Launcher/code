use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use serde_json::Value;

use crate::{Core, CoreError, Permission};

pub type ActionFuture = Pin<Box<dyn Future<Output = Result<Value, CoreError>> + Send>>;
pub type Handler = Arc<dyn Fn(Arc<Core>, Value) -> ActionFuture + Send + Sync>;

#[derive(Clone)]
pub struct ActionSpec {
    pub permission: Permission,
    /// May be bound from theme markup (`data-action`). Such calls are always a
    /// direct user click, never code.
    pub bindable: bool,
    pub handler: Handler,
}

#[derive(Default)]
pub struct ActionRegistry {
    actions: BTreeMap<String, ActionSpec>,
}

impl ActionRegistry {
    pub fn register<F, Fut>(&mut self, name: &str, permission: Permission, bindable: bool, f: F)
    where
        F: Fn(Arc<Core>, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, CoreError>> + Send + 'static,
    {
        assert!(valid_name(name), "invalid action name: {name}");
        let handler: Handler = Arc::new(move |core, args| Box::pin(f(core, args)));
        let prev = self.actions.insert(
            name.to_owned(),
            ActionSpec {
                permission,
                bindable,
                handler,
            },
        );
        assert!(prev.is_none(), "duplicate action: {name}");
    }

    pub fn get(&self, name: &str) -> Option<&ActionSpec> {
        self.actions.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ActionSpec)> {
        self.actions.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// `namespace.verb[.more]`, lowercase ASCII, digits, `_` and `-`.
pub fn valid_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        })
}

/// An addon may only register actions inside its own namespace: `<addon id>.*`.
pub fn addon_may_register(addon_id: &str, action: &str) -> bool {
    valid_name(action)
        && action
            .strip_prefix(addon_id)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Who is calling an action.
#[derive(Clone, Debug)]
pub enum Caller {
    /// The built-in screens. Same API as addons, full grant.
    BuiltinUi,
    /// A user click on a `data-action` element of a theme.
    Theme { id: String },
    /// Addon code with the permissions the user granted at install time.
    Addon {
        id: String,
        granted: HashSet<Permission>,
    },
}

impl Caller {
    pub fn check(&self, action: &str, spec: &ActionSpec) -> Result<(), CoreError> {
        let ok = match self {
            Caller::BuiltinUi => true,
            Caller::Theme { .. } => spec.bindable && spec.permission != Permission::Internal,
            Caller::Addon { granted, .. } => {
                spec.permission == Permission::Public
                    || (spec.permission != Permission::Internal
                        && granted.contains(&spec.permission))
            }
        };
        if ok {
            Ok(())
        } else {
            Err(CoreError::Denied(format!(
                "{action} requires {:?}",
                spec.permission
            )))
        }
    }
}
