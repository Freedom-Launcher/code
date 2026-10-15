use serde::{Deserialize, Serialize};

/// What an action needs from its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Harmless, read-only information (app info, version list).
    Public,
    AccountsRead,
    AccountsManage,
    InstancesRead,
    InstancesManage,
    Network,
    GameLaunch,
    /// Acting as an account provider (handles credentials).
    AccountsProvide,
    /// Spawning arbitrary processes, raw filesystem access.
    Process,
    /// Launcher self-management (switching themes, approving addons, safe
    /// recovery). Built-in UI only; never grantable to an addon.
    Internal,
}

/// Permission tiers shown to the user when installing an extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// «Тема»: visuals only, no code.
    Theme,
    /// «Интерфейс»: UI slots, read-only data.
    Interface,
    /// «Функция»: changes launcher state, network.
    Function,
    /// «Расширенный»: credentials, processes. Official/Verified only.
    Extended,
}

/// Trust badge of an extension package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Official,
    Verified,
    Community,
}

impl Permission {
    pub fn tier(self) -> Tier {
        use Permission::*;
        match self {
            Public => Tier::Theme,
            AccountsRead | InstancesRead => Tier::Interface,
            AccountsManage | InstancesManage | Network | GameLaunch => Tier::Function,
            AccountsProvide | Process | Internal => Tier::Extended,
        }
    }
}

impl Trust {
    /// Highest tier a package with this badge may request.
    pub fn max_tier(self) -> Tier {
        match self {
            Trust::Official | Trust::Verified => Tier::Extended,
            Trust::Community => Tier::Function,
        }
    }
}
