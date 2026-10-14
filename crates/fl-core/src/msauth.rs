//! Microsoft sign-in: OAuth device code → Xbox Live → XSTS → Minecraft services.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{
    accounts::{Account, AccountKind, Secrets},
    unix_now, Core, CoreError,
};

const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const SCOPE: &str = "XboxLive.signin offline_access";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
pub const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

pub struct Pending {
    device_code: String,
    interval: u64,
    expires_at: Instant,
}

/// Azure application (client) ID. Runtime env wins over the build-time value.
pub fn client_id() -> Result<String, CoreError> {
    std::env::var("FREEDOM_MSA_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("FREEDOM_MSA_CLIENT_ID").map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            CoreError::msg("Microsoft client ID is not configured (FREEDOM_MSA_CLIENT_ID)")
        })
}

fn field(v: &Value, key: &str) -> Result<String, CoreError> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CoreError::msg(format!("unexpected response: missing {key}")))
}

pub async fn begin(core: &Core) -> Result<Value, CoreError> {
    let cid = client_id()?;
    let v: Value = core
        .http()
        .post(DEVICE_URL)
        .form(&[("client_id", cid.as_str()), ("scope", SCOPE)])
        .send()
        .await?
        .json()
        .await?;
    if v.get("error").is_some() {
        return Err(CoreError::msg(format!(
            "Microsoft: {}",
            v["error_description"].as_str().unwrap_or("sign-in failed")
        )));
    }
    let expires_in = v["expires_in"].as_u64().unwrap_or(900);
    *core.ms_pending.lock().await = Some(Pending {
        device_code: field(&v, "device_code")?,
        interval: v["interval"].as_u64().unwrap_or(5),
        expires_at: Instant::now() + Duration::from_secs(expires_in),
    });
    Ok(json!({
        "user_code": field(&v, "user_code")?,
        "verification_uri": field(&v, "verification_uri")?,
        "expires_in": expires_in,
    }))
}

/// Waits until the user confirms the code in the browser, then completes the chain.
pub async fn wait(core: &Core) -> Result<Account, CoreError> {
    let p = core
        .ms_pending
        .lock()
        .await
        .take()
        .ok_or_else(|| CoreError::msg("no Microsoft sign-in in progress"))?;
    let cid = client_id()?;
    let mut interval = p.interval.max(1);
    let token = loop {
        if Instant::now() > p.expires_at {
            return Err(CoreError::msg("the code has expired, start sign-in again"));
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let v: Value = core
            .http()
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", DEVICE_GRANT),
                ("client_id", cid.as_str()),
                ("device_code", p.device_code.as_str()),
            ])
            .send()
            .await?
            .json()
            .await?;
        if v.get("access_token").is_some() {
            break v;
        }
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("authorization_declined") => return Err(CoreError::msg("sign-in was declined")),
            Some(e) => return Err(CoreError::msg(format!("Microsoft: {e}"))),
            None => return Err(CoreError::msg("unexpected token response")),
        }
    };
    minecraft_login(
        core,
        &field(&token, "access_token")?,
        field(&token, "refresh_token")?,
    )
    .await
}

async fn minecraft_login(
    core: &Core,
    ms_access: &str,
    refresh_token: String,
) -> Result<Account, CoreError> {
    let http = core.http();

    let xbl: Value = http
        .post(XBL_URL)
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let xbl_token = field(&xbl, "Token")?;

    let resp = http
        .post(XSTS_URL)
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;
    if resp.status().as_u16() == 401 {
        let v: Value = resp.json().await.unwrap_or_default();
        return Err(CoreError::msg(xsts_error(v["XErr"].as_u64())));
    }
    let xsts: Value = resp.error_for_status()?.json().await?;
    let xsts_token = field(&xsts, "Token")?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| CoreError::msg("unexpected Xbox response"))?
        .to_owned();

    let mc: Value = http
        .post(MC_LOGIN_URL)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mc_token = field(&mc, "access_token")?;
    let expires_in = mc["expires_in"].as_u64().unwrap_or(86_400);

    let resp = http
        .get(MC_PROFILE_URL)
        .bearer_auth(&mc_token)
        .send()
        .await?;
    if resp.status().as_u16() == 404 {
        return Err(CoreError::msg(
            "this Microsoft account has no Minecraft: Java Edition profile",
        ));
    }
    let profile: Value = resp.error_for_status()?.json().await?;

    Ok(Account {
        id: uuid::Uuid::new_v4().to_string(),
        kind: AccountKind::Microsoft,
        name: field(&profile, "name")?,
        uuid: field(&profile, "id")?,
        owns_game: true,
        secrets: Some(Secrets {
            refresh_token,
            mc_access_token: mc_token,
            mc_expires_at: unix_now() + expires_in,
        }),
    })
}

fn xsts_error(code: Option<u64>) -> String {
    match code {
        Some(2148916233) => {
            "this Microsoft account has no Xbox profile yet; create one at xbox.com".into()
        }
        Some(2148916235) => "Xbox Live is not available in this account's region".into(),
        Some(2148916236) | Some(2148916237) => {
            "this account needs adult verification on xbox.com".into()
        }
        Some(2148916238) => "child account: an adult must add it to a Microsoft family".into(),
        Some(c) => format!("Xbox sign-in failed (XErr {c})"),
        None => "Xbox sign-in failed".into(),
    }
}
