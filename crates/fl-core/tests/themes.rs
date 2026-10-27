use std::{fs, path::PathBuf};

use fl_core::themes;

fn tmp(files: &[(&str, &str)]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fl-themes-{}", uuid_like()));
    for (p, c) in files {
        let p = d.join("themes").join(p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, c).unwrap();
    }
    fs::create_dir_all(&d).unwrap();
    d
}

fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    )
}

const GOOD: &str = r##"{"id":"night","name":"Ночь","base":"dark",
  "tokens":{"--accent":"#9ecbff","--font-ui":"\"Inter\", sans-serif"},
  "css":["theme.css"],"templates":{"view.instance":"t/instance.html"}}"##;

#[test]
fn good_theme_loads_and_lists() {
    let d = tmp(&[
        ("night/theme.json", GOOD),
        ("night/theme.css", ".hero{background:url(img/bg.webp)}"),
        (
            "night/t/instance.html",
            r##"<div data-slot="hero"><button data-action="game.launch">Играть</button><a href="#x">x</a></div>"##,
        ),
    ]);
    let m = themes::load(&d, "night").unwrap();
    assert_eq!(m.tokens["--accent"], "#9ecbff");
    let l = themes::list(&d);
    assert_eq!(l.len(), 3);
    assert!(l[2].get("error").is_none(), "{:?}", l[2]);
    let cur = themes::current(&d, "night");
    assert_eq!(cur["templates"][0], "view.instance");
    assert!(themes::template(&d, "night", "view.instance")
        .unwrap()
        .is_some());
    assert!(themes::template(&d, "night", "view.create")
        .unwrap()
        .is_none());
}

#[test]
fn bad_css_is_rejected() {
    for css in [
        "a{background:url(https://evil.example/x.png)}",
        "@import 'x.css';",
        "a{background:url(../../secret.png)}",
        "a{b\\61 ckground:u\\72l(http://x)}",
        "a{background:image-set('https://x/y.png' 1x)}",
    ] {
        assert!(themes::check_css(css).is_err(), "{css}");
    }
    assert!(themes::check_css("a{background:url('img/a.png');color:red}").is_ok());
    // Comments are ignored, but cannot be used to split a forbidden word.
    assert!(themes::check_css("/* url() and @import in a comment */ a{color:red}").is_ok());
    assert!(themes::check_css("a{background:u/**/rl(https://x/y.png)}").is_err());
    assert!(themes::check_css("@im/**/port 'x.css';").is_err());
}

#[test]
fn bad_templates_are_rejected() {
    for html in [
        "<script>alert(1)</script>",
        r#"<img src="x.png" onerror="alert(1)">"#,
        r#"<a href="javascript:alert(1)">x</a>"#,
        r#"<a href="https://example.com">x</a>"#,
        r#"<div/onclick=alert(1)>"#,
        "<iframe src=x.html>",
        r#"<svg><animate attributeName="href" to="x"/></svg>"#,
        r#"<a title='x>' onclick=alert(1)>x</a>"#,
        r#"<div style="background:url(https://x)">"#,
    ] {
        assert!(themes::check_template(html).is_err(), "{html}");
    }
    assert!(themes::check_template(
        r#"<img src="img/a.png" alt="one = two"><button data-action="x.y">Да</button>"#
    )
    .is_ok());
}

#[test]
fn tokens_are_checked() {
    assert!(themes::check_token("--accent", "#fff").is_ok());
    assert!(themes::check_token("accent", "#fff").is_err());
    assert!(themes::check_token("--bg", "url(x.png)").is_err());
    assert!(themes::check_token("--bg", "red; } body { display:none").is_err());
}

#[test]
fn broken_or_mismatched_theme_falls_back() {
    let d = tmp(&[
        ("other/theme.json", GOOD), // id "night" in folder "other"
        (
            "bad/theme.json",
            r#"{"id":"bad","name":"B","css":["x.css"]}"#,
        ),
        ("bad/x.css", "@import 'y.css';"),
    ]);
    assert!(themes::load(&d, "other").is_err());
    let cur = themes::current(&d, "bad");
    assert_eq!(cur["id"], "default");
    assert_eq!(cur["fallback"]["theme"], "bad");
    assert!(themes::list(&d)
        .iter()
        .all(|t| t["builtin"] == true || t.get("error").is_some()));
    assert!(themes::file(&d, "bad", "../other/theme.json").is_none());
}
#[test]
fn shipped_example_theme_is_valid() {
    let d = std::env::temp_dir().join(format!("fl-themes-ex-{}", uuid_like()));
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/themes/example");
    let dst = d.join("themes/example");
    fs::create_dir_all(&dst).unwrap();
    for f in ["theme.json", "theme.css", "empty.html"] {
        fs::copy(src.join(f), dst.join(f)).unwrap();
    }
    let m = themes::load(&d, "example").unwrap();
    assert_eq!(m.templates["view.empty"], "empty.html");
}

/// The shipped example themes pass every check and survive a `.fltheme` round trip.
#[test]
fn shipped_themes_load_and_pack() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/themes");
    for id in ["example", "cyberpunk"] {
        let d = tmp(&[]);
        let to = d.join("themes").join(id);
        fs::create_dir_all(&to).unwrap();
        for e in fs::read_dir(src.join(id)).unwrap().flatten() {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
        themes::load(&d, id).unwrap_or_else(|e| panic!("{id}: {e}"));
        let out = d.join(format!("{id}.fltheme"));
        themes::export_zip(&d, id, &out).unwrap();
        fs::remove_dir_all(&to).unwrap();
        assert_eq!(themes::install_zip(&d, &out).unwrap(), id);
        let _ = fs::remove_dir_all(&d);
    }
}
