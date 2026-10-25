//! «Ускорение сборки»: checks an instance's memory, Java flags and the well-known
//! performance mods, and applies the fixes the user picks.

use serde_json::{json, Value};

use crate::{
    game::launch,
    instances::{Instance, InstanceStore},
    modrinth::{self, Kind},
    mods, store, Core, CoreError,
};

/// Aikar's G1 flags, the usual choice for a smooth client with a larger heap.
pub const FLAGS: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:SurvivorRatio=32",
    "-XX:MaxTenuringThreshold=1",
];

fn ours(t: &str) -> bool {
    t.starts_with("-XX:") || t.starts_with("-Xmx") || t.starts_with("-Xms") || t.starts_with("-Xmn")
}

pub fn has_flags(cur: &str) -> bool {
    let t: Vec<&str> = cur.split_whitespace().collect();
    FLAGS.iter().all(|f| t.contains(f))
}

/// Heap sizes belong to the memory setting; in the arguments they fight with it.
pub fn heap_conflict(cur: &str) -> bool {
    cur.split_whitespace().any(|t| t.starts_with("-Xmx") || t.starts_with("-Xms"))
}

/// Keeps the user's own arguments (system properties, agents…) and replaces GC tuning with ours.
pub fn merge_flags(cur: &str) -> String {
    let mut v: Vec<&str> = cur.split_whitespace().filter(|t| !ours(t)).collect();
    v.extend(FLAGS);
    v.join(" ")
}

/// Memory for this many mods, leaving the system room to breathe.
pub fn recommend_mb(total: Option<u64>, mods: usize) -> u32 {
    let want: u64 = match mods {
        0 => 2048,
        1..=40 => 3072,
        41..=120 => 4096,
        121..=220 => 6144,
        _ => 8192,
    };
    let v = match total {
        Some(t) => want.min((t / 2).max(t.saturating_sub(4096))),
        None => want.min(4096),
    };
    (v.max(1024) / 512 * 512) as u32
}

struct Cand {
    id: &'static str,
    slug: &'static str,
    title: &'static str,
    /// Jar name parts that mean the mod (or a fork of it) is already there.
    alias: &'static [&'static str],
    /// Replaces the renderer: does not get along with OptiFine.
    render: bool,
}

const SODIUM: Cand = Cand { id: "AANobbMI", slug: "sodium", title: "Sodium", alias: &["sodium", "embeddium", "rubidium", "magnesium"], render: true };
const EMBEDDIUM: Cand = Cand { id: "sk9rgfiA", slug: "embeddium", title: "Embeddium", alias: &["embeddium", "rubidium", "sodium"], render: true };
const LITHIUM: Cand = Cand { id: "gvQqBUqZ", slug: "lithium", title: "Lithium", alias: &["lithium", "canary", "radium"], render: false };
const FERRITE: Cand = Cand { id: "uXXizFIs", slug: "ferrite-core", title: "FerriteCore", alias: &["ferritecore", "ferrite-core"], render: false };
const CULLING: Cand = Cand { id: "NNAgCjsB", slug: "entityculling", title: "Entity Culling", alias: &["entityculling"], render: false };
const FAST: Cand = Cand { id: "5ZwdcRci", slug: "immediatelyfast", title: "ImmediatelyFast", alias: &["immediatelyfast"], render: false };
const MODERNFIX: Cand = Cand { id: "nmDcB62a", slug: "modernfix", title: "ModernFix", alias: &["modernfix"], render: false };

fn candidates(loader: &str) -> &'static [Cand] {
    match loader {
        "fabric" | "quilt" | "neoforge" => &[SODIUM, LITHIUM, FERRITE, CULLING, FAST, MODERNFIX],
        "forge" => &[EMBEDDIUM, FERRITE, CULLING, FAST, MODERNFIX],
        _ => &[],
    }
}

fn mods_dir_state(inst: &Instance, gd: &std::path::Path) -> (Vec<String>, usize, Vec<String>) {
    if Kind::Mod.supports(&inst.loader, &inst.version).is_none() {
        return (vec![], 0, vec![]);
    }
    let entries = mods::list(gd, Kind::Mod).unwrap_or_default();
    let files = entries.iter().map(|e| e.file.to_lowercase()).collect();
    let count = entries.iter().filter(|e| e.enabled).count();
    let ids = mods::load_index(gd, Kind::Mod)
        .map(|i| i.values().map(|s| s.project_id.clone()).collect())
        .unwrap_or_default();
    (files, count, ids)
}

pub async fn analyze(core: &Core, inst: &Instance) -> Result<Value, CoreError> {
    let gd = inst.game_dir(&core.instances_root());
    let global: crate::settings::Settings = store::load(&core.path("settings.json"))?;
    let merged = inst.settings.over(&global.game);
    let total = launch::total_ram_mb();
    let (files, count, ids) = mods_dir_state(inst, &gd);
    let rec = recommend_mb(total, count);
    let (memory_mb, memory_src) = match (inst.settings.memory_mb, global.game.memory_mb) {
        (Some(m), _) => (m, "instance"),
        (None, Some(m)) => (m, "global"),
        _ => (launch::default_memory_mb(total), "auto"),
    };
    let jvm = merged.jvm_args.clone().unwrap_or_default();
    let flags_ok = has_flags(&jvm) && !heap_conflict(&jvm);
    let cands = candidates(&inst.loader);
    let optifine = files.iter().any(|f| f.contains("optifine") || f.contains("optifabric"));
    let loaders = Kind::Mod.supports(&inst.loader, &inst.version);
    let avail: Vec<bool> = match &loaders {
        Some(l) => futures_util::future::join_all(cands.iter().map(|c| modrinth::versions(core, c.id, &inst.version, l)))
            .await
            .into_iter()
            .map(|r| r.map(|v| !v.is_empty()).unwrap_or(false))
            .collect(),
        None => vec![false; cands.len()],
    };
    let pid: Vec<String> = cands.iter().map(|c| c.id.to_owned()).collect();
    let meta = if cands.is_empty() { vec![] } else { modrinth::projects(core, &pid).await.unwrap_or_default() };
    let (mut eligible, mut have) = (0usize, 0usize);
    let list: Vec<Value> = cands
        .iter()
        .zip(avail)
        .map(|(c, available)| {
            let present = ids.iter().any(|i| i == c.id) || files.iter().any(|f| c.alias.iter().any(|a| f.contains(a)));
            let blocked = c.render && optifine && !present;
            if (available || present) && !blocked {
                eligible += 1;
                have += present as usize;
            }
            let m = meta.iter().find(|p| p.id == c.id);
            json!({
                "id": c.id,
                "slug": c.slug,
                "title": m.map(|p| p.title.as_str()).unwrap_or(c.title),
                "icon_url": m.and_then(|p| p.icon_url.clone()),
                "present": present,
                "available": available,
                "blocked": blocked,
            })
        })
        .collect();
    let mem_ok = memory_mb + 256 >= rec;
    let score = if eligible == 0 {
        (if mem_ok { 50 } else { 0 }) + if flags_ok { 50 } else { 0 }
    } else {
        (if mem_ok { 30 } else { 0 }) + (if flags_ok { 20 } else { 0 }) + 50 * have / eligible
    };
    Ok(json!({
        "instance": inst.id,
        "total_mb": total,
        "memory_mb": memory_mb,
        "memory_src": memory_src,
        "recommended_mb": rec,
        "memory_ok": mem_ok,
        "mods": count,
        "loader": inst.loader,
        "mc": inst.version,
        "jvm_args": jvm,
        "flags_ok": flags_ok,
        "heap_conflict": heap_conflict(&jvm),
        "optifine": optifine,
        "candidates": list,
        "score": score,
    }))
}

pub async fn apply(core: &Core, instance: &str, memory: bool, flags: bool, want: &[String]) -> Result<Value, CoreError> {
    if core.game_running.is_busy(instance) {
        return Err(CoreError::msg("Сначала закройте игру"));
    }
    let inst = {
        let _g = core.state_lock.lock().await;
        let path = core.path("instances.json");
        let mut s: InstanceStore = store::load(&path)?;
        let i = s
            .instances
            .iter_mut()
            .find(|i| i.id == instance)
            .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {instance}")))?;
        if memory || flags {
            let gd = i.game_dir(&core.instances_root());
            let (_, count, _) = mods_dir_state(i, &gd);
            let mut set = i.settings.clone();
            if memory {
                set.memory_mb = Some(recommend_mb(launch::total_ram_mb(), count));
            }
            if flags {
                set.jvm_args = Some(merge_flags(set.jvm_args.as_deref().unwrap_or_default()));
            }
            set.validate()?;
            i.settings = set;
        }
        let out = i.clone();
        store::save(&path, &s, false)?;
        out
    };
    let mut installed = Vec::new();
    let mut failed = Vec::new();
    if !want.is_empty() {
        let loaders = Kind::Mod
            .supports(&inst.loader, &inst.version)
            .ok_or_else(|| CoreError::msg("в Vanilla моды недоступны"))?;
        let gd = inst.game_dir(&core.instances_root());
        let cands = candidates(&inst.loader);
        for id in want {
            let Some(c) = cands.iter().find(|c| c.id == id) else {
                return Err(CoreError::BadArgs(format!("unknown mod: {id}")));
            };
            let t = mods::Target { game_dir: &gd, kind: Kind::Mod, mc: &inst.version, loaders: &loaders };
            match mods::install(core, t, c.id, None).await {
                Ok(a) => installed.extend(a),
                Err(e) => failed.push(json!({ "id": c.id, "title": c.title, "error": e.to_string() })),
            }
        }
    }
    Ok(json!({
        "instance": inst.id,
        "memory_mb": inst.settings.memory_mb,
        "jvm_args": inst.settings.jvm_args,
        "installed": installed,
        "failed": failed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_merge_keeps_own_args() {
        let m = merge_flags("-Dfoo=1 -Xmx2G -XX:+UseZGC -javaagent:x.jar");
        assert!(m.starts_with("-Dfoo=1 -javaagent:x.jar -XX:+UseG1GC"));
        assert!(!m.contains("ZGC") && !m.contains("-Xmx"));
        assert!(has_flags(&m) && !heap_conflict(&m));
        assert_eq!(merge_flags(&m), m);
        assert!(m.len() <= 1024);
        assert!(heap_conflict("-Xms1G") && !has_flags(""));
    }

    #[test]
    fn memory_recommendation() {
        assert_eq!(recommend_mb(Some(16384), 0), 2048);
        assert_eq!(recommend_mb(Some(16384), 30), 3072);
        assert_eq!(recommend_mb(Some(16384), 300), 8192);
        assert_eq!(recommend_mb(Some(8192), 300), 4096);
        assert_eq!(recommend_mb(Some(6000), 300), 2560);
        assert_eq!(recommend_mb(Some(1500), 300), 1024);
        assert_eq!(recommend_mb(None, 300), 4096);
    }

    #[test]
    fn candidates_by_loader() {
        assert_eq!(candidates("fabric")[0].slug, "sodium");
        assert_eq!(candidates("forge")[0].slug, "embeddium");
        assert!(candidates("vanilla").is_empty());
    }
}