// SPDX-License-Identifier: GPL-2.0-or-later
//! Functional check of every menu control (report: local/audit/func-check-report.md).
//! `funccheck -- --ignored` walks every menu, sheet page and tab of the three layouts, presses each control with a real
//! tap and checks what it did: the model, the chosen Ui fields, the toast, the picture, the displayed value and the SCPI
//! header of the option. Results go to local/audit/func/results.tsv (one file per layout, joined by the report step).
use super::*;
use super::sheet::RowKind;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

const OUT_DIR: &str = "/mnt/c/Projects/oss-plus/local/audit/func";

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    u.step(0.05);
}
fn touch(u: &mut Ui, phase: Phase, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase, x, y });
    u.step(0.01);
}
fn tap_at(u: &mut Ui, x: i32, y: i32) {
    touch(u, Phase::Down, x, y);
    touch(u, Phase::Up, x, y);
    frame(u);
}
fn cen(r: Rect) -> (i32, i32) { (r.x + r.w / 2, r.y + r.h / 2) }

fn shot(u: &mut Ui, name: &str) {
    for _ in 0..3 { frame(u); }
    u.toast = None;
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    let dir = std::path::Path::new(OUT_DIR);
    let _ = std::fs::create_dir_all(dir);
    std::fs::write(dir.join(format!("{name}.png")), crate::store::png_of(&cv)).unwrap();
}

// ------------------------------------------------------------------------------------------------ the Views repro

/// Display > Views in the Minimal layout: what the four segments do, to the model and to the screen.
#[test]
#[ignore = "an audit run by hand, like the others here: Display > Views is no longer on the first tab of the sheet"]
fn views_repro() {
    let mut u = build(Layout::Minimal, 900);
    shot(&mut u, "views-0-base");
    let r = u.top_buttons().into_iter().find(|b| b.0 == TopBtn::Menu(MenuId::Display)).unwrap().2;
    let (x, y) = cen(r);
    tap_at(&mut u, x, y);
    let c = controls(&u, 0).into_iter().find(|c| c.it == Item::Views).unwrap();
    for k in [1usize, 2, 3, 0] {
        let before = snap(&mut u, None);
        tap_c(&mut u, &c, &format!("seg{k}"));
        let after = snap(&mut u, None);
        println!("FC views seg{k}: views {} frame px changed {} (outside menu {}) toast '{}'", u.m.views, pdiff(&before.px, &after.px), pdiff_out(&before.px, &after.px, u.scr.w, u.menu_rect()), after.toast);
        shot(&mut u, &format!("views-menu-{}", k + 1));
    }
    tap_c(&mut u, &c, "seg2");
    u.close_menu();
    frame(&mut u);
    shot(&mut u, "views-3-closed");
    let b = u.view_box().unwrap();
    let before = snap(&mut u, None);
    tap_at(&mut u, b.x + 28 + 10, b.y + 15);
    let after = snap(&mut u, None);
    println!("FC tap view 2: cur {} px changed {} toast '{}'", u.m.view_cur, pdiff(&before.px, &after.px), after.toast);
    shot(&mut u, "views-3-view2");
}

// ------------------------------------------------------------------------------------------------ model of a control

#[derive(Clone, Copy, PartialEq, Debug)]
enum CK { Seg, Num, Pick, Toggle, Link, Foot, Text }

#[derive(Clone, Debug)]
struct C {
    it: Item,
    ck: CK,
    rect: Rect,
    opts: Vec<String>,
    sel: Option<usize>,
    tab: usize,
    sheet: bool,
}

struct Row {
    layout: String,
    menu: String,
    variant: String,
    tab: String,
    label: String,
    item: String,
    kind: String,
    op: String,
    verdict: String,
    fields: String,
    frame: String,
    evidence: String,
    scpi: String,
    note: String,
}

impl Row {
    fn tsv(&self) -> String {
        [&self.layout, &self.menu, &self.variant, &self.tab, &self.label, &self.item, &self.kind, &self.op, &self.verdict, &self.fields, &self.frame, &self.evidence, &self.scpi, &self.note]
            .iter().map(|s| s.replace(['\t', '\n', '\r'], " ")).collect::<Vec<_>>().join("\t")
    }
}

/// State seen from outside: the model's top-level fields, chosen Ui fields, the toast and the picture.
struct Snap {
    model: BTreeMap<String, String>,
    ui: BTreeMap<&'static str, String>,
    toast: String,
    px: Vec<u32>,
    value: String,
}

fn model_fields(m: &Model) -> BTreeMap<String, String> {
    let text = format!("{:#?}", m);
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut cur: Option<String> = None;
    for line in text.lines() {
        let b = line.as_bytes();
        let is_top = line.starts_with("    ") && b.len() > 4 && b[4] != b' ' && b[4] != b'}' && b[4] != b')' && b[4] != b']';
        if is_top {
            if let Some((k, v)) = line.trim_start().split_once(": ") {
                out.insert(k.to_string(), v.to_string());
                cur = Some(k.to_string());
                continue;
            }
        }
        if let Some(k) = &cur { if let Some(e) = out.get_mut(k) { e.push_str(line.trim()); } }
    }
    out
}

fn ui_fields(u: &Ui) -> BTreeMap<&'static str, String> {
    let mut v = BTreeMap::new();
    v.insert("menu", format!("{:?}/{}", u.menu, u.menu_open));
    v.insert("pick", format!("{:?}", u.pick.map(|p| p.it)));
    v.insert("xy", format!("{}", u.xy));
    v.insert("full", format!("{}", u.full));
    v.insert("meas_src", format!("{:?}", u.meas_src));
    v.insert("meas_src_b", format!("{:?}", u.meas_src_b));
    v.insert("ref_sel", format!("{}", u.ref_sel));
    v.insert("ref_src", format!("{}", u.ref_src));
    v.insert("hist_ch", format!("{:?}", u.hist_ch));
    v.insert("hist_pos", format!("{:?}", u.hist_pos));
    v.insert("zone_sel", format!("{}", u.zone_sel));
    v.insert("offline", format!("{}", u.offline));
    v.insert("sim_play", format!("{}", u.sim_play));
    v.insert("search_on", format!("{}", u.search_on));
    v.insert("dialog", format!("{:?}", u.dialog));
    v.insert("numpad", format!("{}", u.numpad.is_some()));
    v.insert("store_slot", format!("{}", u.store_slot));
    v.insert("shot_request", format!("{:?}", u.shot_request));
    v.insert("power_dlg", format!("{}", u.power_dlg));
    v.insert("panel_tx", format!("{}", u.panel_tx.len()));
    v.insert("table", format!("{}", u.table.is_some()));
    v.insert("cat", format!("{}/{}/{}", u.cat.tab, u.cat.sel, u.cat.scroll));
    v.insert("single_armed", format!("{}", u.single_armed));
    v.insert("cleared", format!("{}", u.cleared));
    v.insert("refs", format!("{:?}", u.refs.iter().map(|r| r.as_ref().map(|r| (r.on, r.color))).collect::<Vec<_>>()));
    v.insert("mask", format!("{}", u.mask.is_some()));
    v.insert("offline_ref", format!("{}", u.offline_ref));
    v.insert("auto_run", format!("{}", u.auto.is_some()));
    v
}

fn snap(u: &mut Ui, it: Option<Item>) -> Snap {
    let toast = u.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default();
    u.toast = None;
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    Snap { model: model_fields(&u.m), ui: ui_fields(u), toast, px: cv.px, value: it.map(|i| u.value(i)).unwrap_or_default() }
}

fn pdiff(a: &[u32], b: &[u32]) -> usize { a.iter().zip(b).filter(|(x, y)| x != y).count() }
fn pdiff_out(a: &[u32], b: &[u32], w: i32, r: Option<Rect>) -> usize {
    let mut n = 0;
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x == y { continue; }
        let (px, py) = (i as i32 % w, i as i32 / w);
        if r.is_some_and(|r| r.contains(px, py)) { continue; }
        n += 1;
    }
    n
}

fn cut(s: &str) -> String {
    let t: String = s.chars().take(48).collect();
    if s.chars().count() > 48 { format!("{t}..") } else { t }
}

/// What changes by itself between two snapshots with no input.
#[derive(Default)]
struct Drift {
    keys: Vec<String>,
    px_all: usize,
    px_out: usize,
}

struct Outcome {
    mf: Vec<String>,
    uf: Vec<String>,
    frame: bool,
    bg: bool,
    toast: String,
    v0: String,
    v1: String,
    panic: Option<String>,
}

impl Outcome {
    fn observable(&self) -> bool { !self.mf.is_empty() || !self.uf.is_empty() || !self.toast.is_empty() || self.frame }
    fn fields(&self) -> String {
        let mut v: Vec<String> = self.mf.iter().take(6).cloned().collect();
        v.extend(self.uf.iter().take(4).cloned());
        if self.mf.len() > 6 { v.push(format!("(+{} more)", self.mf.len() - 6)); }
        v.join(" | ")
    }
    fn frame_s(&self) -> String { format!("{}{}", if self.frame { "y" } else { "n" }, if self.bg { "+bg" } else { "" }) }
}

fn lname(l: Layout) -> &'static str { match l { Layout::Default => "Default", Layout::Minimal => "Minimal", Layout::Studio => "Studio" } }

fn build(l: Layout, n: usize) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = l;
    for i in 0..NCH { u.m.ch[i].on = true; }
    if l == Layout::Studio { u.studio.enter = true; }
    let dir = std::env::temp_dir().join(format!("oos-func-{}-{}", std::process::id(), n % 40));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    u.store_dir = dir;
    for _ in 0..10 { u.step(0.05); }
    u.m.run = RunState::Stop;
    for _ in 0..3 { frame(&mut u); }
    u
}

/// Hardware-dangerous: listed, never pressed.
fn skipped(menu: MenuId, it: Item, label: &str) -> Option<&'static str> {
    let l = label.to_lowercase();
    match it {
        Item::PowerAct => Some("power off / restart"),
        Item::Update | Item::UpdateRollback => Some("firmware update / rollback"),
        Item::BootMode => Some("boot mode"),
        Item::Overclock => Some("overclock"),
        Item::Stock => Some("stock firmware"),
        _ => {
            if (l.contains("format") && menu == MenuId::Storage) || l.contains("erase") || l.contains("factory") { Some("format / erase") }
            else if l.contains("power off") || l.contains("restart") || l.contains("reboot") || l.contains("shut down") { Some("power off / restart") }
            else { None }
        }
    }
}

fn destructive_item(it: Item) -> bool {
    matches!(it, Item::SpecLimitClear | Item::EyeClear | Item::MaskClear | Item::SimDefaults | Item::CurRemove | Item::SrchRemove | Item::RefClear | Item::MeasClear | Item::ClearDisp | Item::DefaultAct)
}

fn tab_rects(u: &Ui) -> Vec<Rect> {
    if u.sheet_active() { u.sheet_geo().map(|g| g.tabs).unwrap_or_default() } else { Vec::new() }
}

/// Every control of the page on screen (the sheet's current tab, or the classic dialog).
fn controls(u: &Ui, tab: usize) -> Vec<C> {
    let mut v = Vec::new();
    if u.sheet_active() {
        if u.menu == MenuId::Main {
            let g = u.menu_geo();
            for grp in &g.groups {
                for &(it, r) in &grp.items { v.push(C { it, ck: CK::Link, rect: r, opts: vec![], sel: None, tab, sheet: true }); }
            }
        } else if let Some(g) = u.sheet_geo() {
            for r in &g.rows {
                let (opts, sel) = u.sheet_choice(r.it);
                let ck = match r.kind { RowKind::Choice => CK::Seg, RowKind::Pick => CK::Pick, RowKind::Number => CK::Num, RowKind::Toggle => CK::Toggle, RowKind::Link => CK::Link, RowKind::Info => continue };
                let (opts, sel) = if ck == CK::Toggle { (vec!["Off".to_string(), "On".to_string()], None) } else { (opts, sel) };
                v.push(C { it: r.it, ck, rect: r.ctrl, opts, sel, tab, sheet: true });
            }
            for (it, rc) in &g.foot {
                v.push(C { it: *it, ck: CK::Foot, rect: *rc, opts: vec![], sel: None, tab, sheet: true });
            }
        }
    } else if let Some(d) = u.dlg_layout() {
        for r in &d.rows {
            let (ck, opts, sel) = match &r.ctl {
                Ctl::Seg(o, s) => (CK::Seg, o.clone(), *s),
                Ctl::Value => (CK::Num, vec![], None),
                Ctl::Pick => (CK::Pick, u.options(r.it), None),
                Ctl::Button => (CK::Link, vec![], None),
                Ctl::Text => (CK::Text, vec![], None),
            };
            v.push(C { it: r.it, ck, rect: r.ctrl, opts, sel, tab, sheet: false });
        }
    }
    v
}

fn find(u: &Ui, c: &C) -> Option<C> {
    controls(u, c.tab).into_iter().find(|x| x.it == c.it)
}

fn tap_c(u: &mut Ui, c: &C, part: &str) {
    let r = c.rect;
    let (x, y) = match part {
        "minus" => cen(Ui::value_parts(r).0),
        "plus" => cen(Ui::value_parts(r).2),
        "mid" => cen(Ui::value_parts(r).1),
        p if p.starts_with("seg") => {
            let k: i32 = p[3..].parse().unwrap_or(0);
            let n = c.opts.len().max(1) as i32;
            (r.x + (2 * k + 1) * r.w / (2 * n), r.y + r.h / 2)
        }
        _ => cen(r),
    };
    tap_at(u, x, y);
}

struct Ctx {
    layout: Layout,
    menu: MenuId,
    variant: String,
    pre: Vec<(Item, usize)>,
    scpi_map: Vec<(String, String)>,
    ui_n: usize,
}

fn fresh_for(ctx: &mut Ctx) -> Ui {
    ctx.ui_n += 1;
    let mut u = build(ctx.layout, ctx.ui_n);
    for (it, k) in ctx.pre.clone() {
        let o = u.options(it);
        if let Some(w) = o.get(k) { u.select_option(it, w); }
    }
    if ctx.variant == "pane=trend" { u.m.pane = tables::PANE_TREND; }
    if ctx.variant == "ref-stored" { u.activate(Item::RefSave); }
    for _ in 0..2 { frame(&mut u); }
    u.open_menu(ctx.menu);
    frame(&mut u);
    u
}

fn reopen(u: &mut Ui, ctx: &Ctx, tab: usize) {
    u.numpad = None;
    u.dialog = None;
    u.power_dlg = false;
    if !(u.menu_open && u.menu == ctx.menu) {
        u.open_menu(ctx.menu);
        frame(u);
    }
    let tabs = tab_rects(u);
    if let Some(t) = tabs.get(tab) { let (x, y) = cen(*t); tap_at(u, x, y); }
}

fn scpi_q(u: &mut Ui, cmd: &str) -> String {
    let cv = Canvas::new(u.scr.w, u.scr.h);
    let mut s = crate::scpi::Scpi::open(0);
    let r = s.exec_line(u, &cv, cmd);
    String::from_utf8_lossy(&r).trim().to_string()
}

fn norm(s: &str) -> String { s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase() }

/// (normalised menu-equivalent leaf, query command) from docs/scpi/*.md.
fn load_scpi_map() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/scpi");
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else { return out };
    for e in rd.flatten() {
        let Ok(t) = std::fs::read_to_string(e.path()) else { continue };
        for line in t.lines() {
            let cols: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
            if cols.len() < 7 || !cols[1].starts_with('`') || !cols[2].contains("Query") { continue; }
            let menu = cols[5];
            if menu == "-" || menu.is_empty() { continue; }
            let mut cmd = cols[1].trim_matches('`').to_string();
            cmd = cmd.split_whitespace().next().unwrap_or("").to_string();
            while let (Some(a), Some(b)) = (cmd.find('['), cmd.find(']')) { if b > a { cmd.replace_range(a..=b, ""); } else { break; } }
            cmd = cmd.replace("<n>", "1").replace("<m>", "1").replace("<x>", "1");
            if cmd.contains('<') || cmd.contains('{') || cmd.is_empty() { continue; }
            for alt in menu.split(',') {
                let leaf = alt.rsplit('>').next().unwrap_or(alt);
                let pre = norm(menu.split('>').next().unwrap_or(""));
                for l in leaf.split('/') { out.push((format!("{}|{}", pre, norm(l)), format!("{}?", cmd))); }
            }
        }
    }
    out
}

fn scpi_for(ctx: &Ctx, title: &str, label: &str, it: Item) -> Vec<String> {
    let manual: &[(Item, &str)] = &[
        (Item::Persist, ":DISPlay:GRADing:TIME?"), (Item::ColorGrade, ":DISPlay:COLor?"), (Item::Intensity, ":DISPlay:WBRightness?"),
        (Item::GridStyle, ":DISPlay:GRID?"), (Item::GridBright, ":DISPlay:GBRightness?"), (Item::Dots, ":DISPlay:TYPE?"),
        (Item::Hdmi, ":DISPlay:HDMI?"), (Item::Backlight, ":DISPlay:BACKlight?"), (Item::TextSize, ":DISPlay:TEXTsize?"),
        (Item::Layout, ":DISPlay:LAYout?"), (Item::Views, ":DISPlay:VIEW:COUNt?"), (Item::TrendSel(0), ":DISPlay:TRENd1?"), (Item::TrendSel(1), ":DISPlay:TRENd2?"),
    ];
    let mut v: Vec<String> = manual.iter().filter(|m| m.0 == it).map(|m| m.1.to_string()).collect();
    if v.is_empty() {
        let n = norm(label);
        if n.len() >= 3 {
            let t = norm(title);
            for (key, cmd) in &ctx.scpi_map {
                let (pre, leaf) = key.split_once('|').unwrap_or(("", key));
                if !(pre.contains(&t) || (t.len() >= 4 && t.contains(pre))) { continue; }
                if leaf.len() >= 3 && (leaf == n.as_str() || (n.len() >= 5 && leaf.contains(&n)) || (leaf.len() >= 5 && n.contains(leaf))) && !v.contains(cmd) { v.push(cmd.clone()); }
            }
        }
    }
    v.truncate(2);
    v
}

fn first_num(s: &str) -> Option<f64> {
    let t = s.trim().trim_start_matches(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.'));
    let e = t.char_indices().find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*i == 0 && *c == '-'))).map(|x| x.0).unwrap_or(t.len());
    let n: f64 = t[..e].parse().ok()?;
    let rest = t[e..].trim_start();
    let mul = match rest.chars().next() { Some('k') => 1e3, Some('M') => 1e6, Some('G') => 1e9, Some('m') => 1e-3, Some('u') | Some('\u{b5}') => 1e-6, Some('n') => 1e-9, Some('p') => 1e-12, _ => 1.0 };
    Some(n * mul)
}

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_uppercase() && !cur.is_empty() && !cur.chars().last().unwrap().is_ascii_uppercase() { out.push(std::mem::take(&mut cur).to_lowercase()); }
        if ch.is_ascii_alphanumeric() { cur.push(ch); } else if !cur.is_empty() { out.push(std::mem::take(&mut cur).to_lowercase()); }
    }
    if !cur.is_empty() { out.push(cur.to_lowercase()); }
    out.into_iter().filter(|w| w.len() >= 3).collect()
}

/// Whether one of the changed model fields has a name that fits the item or its label.
fn target_fits(item: &str, label: &str, mf: &[String]) -> bool {
    if mf.is_empty() { return true; }
    let mut ws = words(item);
    ws.extend(words(label));
    let alias: &[(&str, &str)] = &[("persist", "persist"), ("colorgrade", "grade"), ("intensity", "intens"), ("backlight", "backlight"), ("zinput", "zsrc"), ("grid", "grid"), ("dots", "dots"), ("hdmi", "hdmi"), ("layout", "layout"), ("textsize", "text_size"), ("views", "view"), ("clear", "clear")];
    for f in mf {
        let name = f.split(':').next().unwrap_or("").to_lowercase();
        let flat = name.replace('_', "");
        for w in &ws {
            if flat.contains(w.as_str()) || (flat.len() >= 3 && w.contains(&flat)) { return true; }
        }
        for (a, b) in alias { if ws.iter().any(|w| w == a) && flat.contains(&b.replace('_', "")) { return true; } }
    }
    false
}

const ERR_WORDS: [&str; 9] = ["error", "fail", "invalid", "cannot", "can't", "not available", "unsupported", "not supported", "unknown"];

struct Walker {
    ctx: Ctx,
    rows: Vec<Row>,
    skipped: Vec<String>,
}

impl Walker {
    fn base(&self, u: &Ui, c: &C, tabname: &str) -> Row {
        Row {
            layout: lname(self.ctx.layout).into(),
            menu: format!("{:?}", self.ctx.menu),
            variant: self.ctx.variant.clone(),
            tab: tabname.into(),
            label: u.item_label(c.it).into(),
            item: format!("{:?}", c.it),
            kind: format!("{:?}", c.ck),
            op: String::new(),
            verdict: "OK".into(),
            fields: String::new(),
            frame: String::new(),
            evidence: String::new(),
            scpi: String::new(),
            note: String::new(),
        }
    }

    fn push(&mut self, mut r: Row, verdict: &str, op: &str, o: Option<&Outcome>, evidence: String) {
        r.verdict = verdict.into();
        r.op = op.into();
        if let Some(o) = o { r.fields = o.fields(); r.frame = o.frame_s(); if !o.toast.is_empty() { r.note = format!("toast '{}'", cut(&o.toast)); } }
        r.evidence = evidence;
        self.rows.push(r);
    }

    /// Runs one operation on the Ui and reports what changed (a panic is caught).
    fn probe(&self, u: &mut Ui, c: &C, drift: &Drift, op: &mut dyn FnMut(&mut Ui)) -> Outcome {
        let f0 = u.focus;
        let (m0, p0) = (u.menu, u.pick.is_some());
        let before = snap(u, Some(c.it));
        let res = catch_unwind(AssertUnwindSafe(|| op(u)));
        let panic = res.err().map(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default());
        if panic.is_none() && u.menu == m0 && u.pick.is_some() == p0 { u.focus = f0; }
        let after = if panic.is_some() { snap(u, Some(c.it)) } else { snap(u, Some(c.it)) };
        let mut mf = Vec::new();
        for (k, v) in &after.model {
            if before.model.get(k) != Some(v) && !drift.keys.contains(k) {
                mf.push(format!("{k}: {} -> {}", cut(&before.model.get(k).cloned().unwrap_or_default()), cut(v)));
            }
        }
        let mut uf = Vec::new();
        for (k, v) in &after.ui {
            if before.ui.get(k) != Some(v) && !drift.keys.contains(&format!("ui.{k}")) {
                uf.push(format!("ui.{k}: {} -> {}", cut(before.ui.get(k).map(|s| s.as_str()).unwrap_or("")), cut(v)));
            }
        }
        let rect = u.menu_rect();
        let d_all = pdiff(&before.px, &after.px);
        let d_out = pdiff_out(&before.px, &after.px, u.scr.w, rect);
        Outcome { mf, uf, frame: d_all > drift.px_all + 40, bg: d_out > drift.px_out + 40, toast: after.toast.clone(), v0: before.value, v1: after.value, panic }
    }

    fn drift(&self, u: &mut Ui, c: &C) -> Drift {
        let a = snap(u, Some(c.it));
        u.step(0.01);
        u.step(0.01);
        frame(u);
        let b = snap(u, Some(c.it));
        let mut keys = Vec::new();
        for (k, v) in &b.model { if a.model.get(k) != Some(v) { keys.push(k.clone()); } }
        for (k, v) in &b.ui { if a.ui.get(k) != Some(v) { keys.push(format!("ui.{k}")); } }
        Drift { keys, px_all: pdiff(&a.px, &b.px), px_out: pdiff_out(&a.px, &b.px, u.scr.w, u.menu_rect()) }
    }

    fn scpi_state(&self, u: &mut Ui, cmds: &[String]) -> Vec<String> {
        cmds.iter().map(|c| scpi_q(u, c)).collect()
    }

    fn verdict_common(&self, o: &Outcome) -> Option<(&'static str, String)> {
        if let Some(p) = &o.panic { return Some(("ERROR", format!("panic: {}", cut(p)))); }
        let t = o.toast.to_lowercase();
        if ERR_WORDS.iter().any(|w| t.contains(w)) { return Some(("ERROR", format!("toast '{}'", cut(&o.toast)))); }
        None
    }

    fn test_control(&mut self, u: &mut Ui, c: &C, tabname: &str) {
        let label = u.item_label(c.it).to_string();
        let item = format!("{:?}", c.it);
        let b = self.base(u, c, tabname);
        if let Some(why) = skipped(self.ctx.menu, c.it, &label) {
            self.skipped.push(format!("{} | {:?} | {} | {} | {}", lname(self.ctx.layout), self.ctx.menu, tabname, label, why));
            self.push(b, "SKIPPED", "-", None, why.to_string());
            return;
        }
        if c.ck == CK::Text {
            self.push(b, "STATIC", "-", None, format!("value '{}'", cut(&u.value(c.it))));
            return;
        }
        if std::env::var("FUNC_TRACE").is_ok() { use std::io::Write; if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::path::Path::new(OUT_DIR).join("trace.txt")) { let _ = writeln!(f, "{:?} {:?} {:?} {}", self.ctx.menu, self.ctx.variant, c.it, label); } }
        let drift = self.drift(u, c);
        let scpi_cmds = scpi_for(&self.ctx, &u.menu_title(self.ctx.menu), &label, c.it);
        let mk = |me: &Walker, u: &Ui| me.base(u, c, tabname);
        match c.ck {
            CK::Seg | CK::Toggle => {
                let cur0 = u.value(c.it);
                let (opts, sel0) = if c.ck == CK::Toggle { (vec!["Off".to_string(), "On".to_string()], if cur0 == "On" { Some(1) } else { Some(0) }) } else { (c.opts.clone(), c.sel) };
                if opts.is_empty() { self.push(b, "STATIC", "-", None, "no options".into()); return; }
                let n = opts.len();
                let mut ks: Vec<usize> = (0..n).collect();
                if n > 6 { let s = sel0.unwrap_or(0); ks = vec![0, 1, n / 2, n - 1, (s + 1) % n, (s + n - 1) % n]; ks.sort(); ks.dedup(); }
                if c.ck == CK::Toggle { ks = vec![1 - sel0.unwrap_or(0)]; }
                let q0 = self.scpi_state(u, &scpi_cmds);
                let mut any_scpi_change = false;
                let mut any_value_change = false;
                for k in ks {
                    if Some(k) == sel0 && c.ck != CK::Toggle { continue; }
                    let c2 = find(u, c).unwrap_or_else(|| c.clone());
                    let part = if c.ck == CK::Toggle { "tap".to_string() } else { format!("seg{k}") };
                    let want = opts[k].clone();
                    let mut o = self.probe(u, &c2, &drift, &mut |u| tap_c(u, &c2, &part));
                    let shown = u.value(c.it);
                    let ok_shown = shown.eq_ignore_ascii_case(&want) || find(u, c).map(|x| x.sel == Some(k)).unwrap_or(false) || c.it == Item::TrigType;
                    let mut r = mk(self, u);
                    r.kind = format!("{:?}", c.ck);
                    let mut v = "OK";
                    let mut ev = format!("{} -> {} (wanted '{}', shows '{}')", o.v0, o.v1, want, shown);
                    if let Some((vv, e)) = self.verdict_common(&o) { v = vv; ev = e; }
                    else if !o.observable() { v = "DEAD"; }
                    else if !ok_shown && c.it != Item::Layout {
                        v = if o.mf.is_empty() && o.uf.is_empty() { "DEAD" } else { "STALE-LABEL" };
                    } else if o.v0 == o.v1 && !o.mf.is_empty() && !ok_shown { v = "STALE-LABEL"; }
                    else if !target_fits(&item, &label, &o.mf) && !o.mf.is_empty() { v = "WRONG-TARGET"; }
                    if o.v0 != o.v1 { any_value_change = true; }
                    let q1 = self.scpi_state(u, &scpi_cmds);
                    if q1 != q0 { any_scpi_change = true; }
                    if !scpi_cmds.is_empty() { r.scpi = format!("{} {:?} -> {:?}", scpi_cmds.join(","), q0, q1); }
                    if u.m.layout != self.ctx.layout { o.mf.push("layout changed (expected for Layout row)".into()); }
                    self.push(r, v, &format!("select '{}'", want), Some(&o), ev);
                    self.recover(u, c.tab, &mut None);
                }
                // restore
                if c.ck == CK::Toggle || sel0.is_some() {
                    let c2 = find(u, c).unwrap_or_else(|| c.clone());
                    let back = if c.ck == CK::Toggle { "tap".to_string() } else { format!("seg{}", sel0.unwrap()) };
                    let now = u.value(c.it);
                    if now != cur0 || c.ck == CK::Toggle {
                        let o = self.probe(u, &c2, &drift, &mut |u| tap_c(u, &c2, &back));
                        let after = u.value(c.it);
                        if after != cur0 {
                            let r = mk(self, u);
                            self.push(r, "NOT-REVERSIBLE", "restore", Some(&o), format!("started '{}', after restore '{}'", cur0, after));
                        }
                        self.recover(u, c.tab, &mut None);
                    }
                }
                if !scpi_cmds.is_empty() && any_value_change {
                    let mut r = mk(self, u);
                    r.scpi = format!("{} start {:?}", scpi_cmds.join(","), q0);
                    if !any_scpi_change && !q0.iter().all(|s| s.is_empty()) {
                        self.push(r, "SCPI-MISMATCH", "scpi", None, "control changed its value but the SCPI query never changed".into());
                    }
                }
            }
            CK::Pick if c.sheet => { self.picker(u, c, tabname, &drift); }
            CK::Num | CK::Pick => {
                let v0 = u.value(c.it);
                let q0 = self.scpi_state(u, &scpi_cmds);
                // minus then plus, with real taps
                let c2 = c.clone();
                let o1 = self.probe(u, &c2, &drift, &mut |u| tap_c(u, &c2, "minus"));
                let v1 = u.value(c.it);
                let q1 = self.scpi_state(u, &scpi_cmds);
                let c3 = find(u, c).unwrap_or_else(|| c.clone());
                let o2 = self.probe(u, &c3, &drift, &mut |u| tap_c(u, &c3, "plus"));
                let v2 = u.value(c.it);
                let q2 = self.scpi_state(u, &scpi_cmds);
                let (n0, n1, n2) = (first_num(&v0), first_num(&v1), first_num(&v2));
                for (op, o, a, bb, na, nb, dir) in [("minus", &o1, &v0, &v1, n0, n1, -1.0), ("plus", &o2, &v1, &v2, n1, n2, 1.0)] {
                    let mut r = mk(self, u);
                    let mut v = "OK";
                    let mut ev = format!("{} -> {}", a, bb);
                    let mut note = String::new();
                    if let Some((vv, e)) = self.verdict_common(o) { v = vv; ev = e; }
                    else if !o.observable() && a == bb { if op == "minus" && v2 != v1 { note = "at the lower limit; plus moves it".into(); } else { v = "DEAD"; note = "no change".into(); } }
                    else if !target_fits(&item, &label, &o.mf) && !o.mf.is_empty() { v = "WRONG-TARGET"; }
                    else if a == bb && !o.mf.is_empty() { v = "STALE-LABEL"; }
                    else if let (Some(x), Some(y)) = (na, nb) {
                        if a != bb && (y - x) * dir < 0.0 { v = "BAD-STEP"; note = "wrong direction".into(); }
                        else if x != 0.0 && ((y - x) / x).abs() > 4.0 { v = "BAD-STEP"; note = format!("step x{:.1}", y / x); }
                    }
                    if !scpi_cmds.is_empty() { r.scpi = format!("{} {:?} -> {:?}", scpi_cmds.join(","), if op == "minus" { &q0 } else { &q1 }, if op == "minus" { &q1 } else { &q2 }); }
                    r.note = note;
                    self.push(r, v, op, Some(o), ev);
                }
                // + then - returns to the start
                if v2 != v0 && !(v1 == v0) {
                    let mut r = mk(self, u);
                    r.scpi = String::new();
                    self.push(r, "NOT-REVERSIBLE", "minus,plus", None, format!("{} -> {} -> {}", v0, v1, v2));
                }
                if v1 == v0 && v2 == v0 {
                    // both at a limit?: leave
                }
                if !scpi_cmds.is_empty() && v1 != v0 && q0 == q1 && q1 == q2 && !q0.iter().all(|s| s.is_empty()) {
                    let mut r = mk(self, u);
                    r.scpi = format!("{} {:?}", scpi_cmds.join(","), q0);
                    self.push(r, "SCPI-MISMATCH", "scpi", None, "value changed but the SCPI query did not".into());
                }
                // limits: keep pressing + and then - with adjust (the handler of the tap)
                self.limits(u, c, &v0, &mk(self, u));
                // the value field
                if c.ck == CK::Num || c.ck == CK::Pick {
                    let c4 = find(u, c).unwrap_or_else(|| c.clone());
                    let was = (u.menu, u.numpad.is_some());
                    let o = self.probe(u, &c4, &drift, &mut |u| tap_c(u, &c4, "mid"));
                    let what = if u.numpad.is_some() { "numpad" } else if u.menu == MenuId::Pick { "picker" } else if o.observable() { "other" } else { "nothing" };
                    let mut r = mk(self, u);
                    r.note = format!("value field tap opens: {what}");
                    let mut v = if o.panic.is_some() { "ERROR" } else { "OK" };
                    if what == "nothing" && !c.sheet && c.ck == CK::Num { v = "NO-VALUE-ENTRY"; }
                    let _ = was;
                    self.push(r, v, "mid", Some(&o), format!("tap on value -> {what}"));
                    self.recover(u, c.tab, &mut None);
                }
                // the picker: choose a different option
                if c.ck == CK::Pick {
                    self.picker(u, c, tabname, &drift);
                }
            }
            CK::Link | CK::Foot => {
                let menu0 = u.menu;
                let c2 = c.clone();
                let twice = c.ck == CK::Foot && destructive_item(c.it);
                let q0 = self.scpi_state(u, &scpi_cmds);
                let mut o = self.probe(u, &c2, &drift, &mut |u| { tap_c(u, &c2, "tap"); if twice { tap_c(u, &c2, "tap"); } });
                let to = u.menu;
                let opened_menu = !(u.menu == menu0 && u.menu_open) && u.menu_open;
                let mut r = mk(self, u);
                let mut v = "OK";
                let mut ev = String::new();
                if let Some((vv, e)) = self.verdict_common(&o) { v = vv; ev = e; }
                else if let Item::Open(m) = c.it {
                    if !(u.menu_open && (to == m || u.menu_root() == m)) { v = "WRONG-TARGET"; ev = format!("opens {:?} instead of {:?}", to, m); }
                    else {
                        let n = controls(u, 0).len();
                        ev = format!("opens {:?}, {} controls", to, n);
                        if n == 0 && !u.sheet_active() { v = "DEAD"; ev = format!("opens {:?} with no controls", to); }
                    }
                } else if opened_menu {
                    ev = format!("navigates to {:?}", to);
                } else if !o.observable() { v = "DEAD"; ev = "nothing changed (no field, no toast, no picture)".into(); }
                else if !target_fits(&item, &label, &o.mf) && !o.mf.is_empty() && !matches!(c.it, Item::AddMeas | Item::DefaultAct | Item::AutosetAct | Item::ClearDisp | Item::RunStop | Item::SingleAct | Item::ForceAct | Item::SlopeAct) { v = "WRONG-TARGET"; ev = "changed fields do not match the label".into(); }
                if ev.is_empty() { ev = format!("changed {} fields, {} ui fields", o.mf.len(), o.uf.len()); }
                if !scpi_cmds.is_empty() { let q1 = self.scpi_state(u, &scpi_cmds); r.scpi = format!("{} {:?} -> {:?}", scpi_cmds.join(","), q0, q1); }
                o.mf.truncate(8);
                self.push(r, v, "tap", Some(&o), ev);
                self.recover(u, c.tab, &mut None);
            }
            CK::Text => {}
        }
    }

    /// Presses + (then -) up to 200 times: it has to stop at a limit or wrap, and come back.
    fn limits(&mut self, u: &mut Ui, c: &C, v0: &str, base: &Row) {
        if c.it == Item::EyePll {
            let mut row = Row { layout: base.layout.clone(), menu: base.menu.clone(), variant: base.variant.clone(), tab: base.tab.clone(), label: base.label.clone(), item: base.item.clone(), kind: base.kind.clone(), op: "limit plus".into(), verdict: "ERROR".into(), fields: String::new(), frame: String::new(), evidence: "stepping PLL bandwidth above 100 kHz aborts the process (allocation over the 9 GB limit); limit test not run".into(), scpi: String::new(), note: "found in the Studio walk".into() };
            row.op = "limit plus".into();
            self.rows.push(row);
            return;
        }
        for (dir, name) in [(1, "plus"), (-1, "minus")] {
            let mut seen: Vec<String> = vec![u.value(c.it)];
            let mut eff = 0;
            let mut state = "runaway";
            let t = std::time::Instant::now();
            let r = catch_unwind(AssertUnwindSafe(|| {
                for i in 0..(if self.ctx.menu == MenuId::Eye { 12 } else { std::env::var("FUNC_CAP").ok().and_then(|s| s.parse().ok()).unwrap_or(200) }) {
                    if std::env::var("FUNC_TRACE").is_ok() { use std::io::Write; if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::path::Path::new(OUT_DIR).join("trace.txt")) { let _ = writeln!(f, "  limit step {i} dir {dir} value {}", u.value(c.it)); } }
                    let before = u.value(c.it);
                    u.adjust(c.it, dir);
                    let after = u.value(c.it);
                    if after == before { return "limit"; }
                    if seen.contains(&after) { return "wraps"; }
                    eff += 1;
                    seen.push(after);
                    if t.elapsed().as_secs() > 40 { return "slow"; }
                }
                "runaway"
            }));
            let _ = &mut state;
            let state = match r { Ok(s) => s, Err(_) => "panic" };
            let end = u.value(c.it);
            let mut row = Row { layout: base.layout.clone(), menu: base.menu.clone(), variant: base.variant.clone(), tab: base.tab.clone(), label: base.label.clone(), item: base.item.clone(), kind: base.kind.clone(), op: format!("limit {name}"), verdict: "OK".into(), fields: String::new(), frame: String::new(), evidence: String::new(), scpi: String::new(), note: String::new() };
            row.evidence = format!("{} steps to {} ('{}' -> '{}')", eff, state, v0, end);
            match state {
                "runaway" | "slow" => { row.verdict = "NO-LIMIT".into(); row.note = format!("{}", seen.iter().rev().take(3).cloned().collect::<Vec<_>>().join(" / ")); }
                "panic" => row.verdict = "ERROR".into(),
                _ => {}
            }
            self.rows.push(row);
            // back with the opposite button: it has to return to where this direction started
            let start = seen[0].clone();
            for _ in 0..eff { u.adjust(c.it, -dir); }
            let back = u.value(c.it);
            if back != start && state != "wraps" {
                let mut row = Row { layout: base.layout.clone(), menu: base.menu.clone(), variant: base.variant.clone(), tab: base.tab.clone(), label: base.label.clone(), item: base.item.clone(), kind: base.kind.clone(), op: format!("limit {name}, return"), verdict: "NOT-REVERSIBLE".into(), fields: String::new(), frame: String::new(), evidence: String::new(), scpi: String::new(), note: String::new() };
                row.evidence = format!("start '{}', {} steps, returned '{}'", start, eff, back);
                self.rows.push(row);
                // put it back as well as possible
                for _ in 0..eff { u.adjust(c.it, -dir); }
            }
        }
        // bring it back to where this started
        let mut guard = 0;
        while u.value(c.it) != v0 && guard < 60 { guard += 1; let b = u.value(c.it); u.adjust(c.it, -1); if u.value(c.it) == b { break; } }
        guard = 0;
        while u.value(c.it) != v0 && guard < 60 { guard += 1; let b = u.value(c.it); u.adjust(c.it, 1); if u.value(c.it) == b { break; } }
    }

    fn picker(&mut self, u: &mut Ui, c: &C, tabname: &str, drift: &Drift) {
        let it = c.it;
        let cur = u.value(it);
        let opts = u.options(it);
        if opts.len() < 2 { return; }
        let mut base = self.base(u, c, tabname);
        base.op = "picker".into();
        let c2 = find(u, c).unwrap_or_else(|| c.clone());
        // open it
        let o = self.probe(u, &c2, drift, &mut |u| { if c2.sheet { tap_c(u, &c2, "tap") } else { tap_c(u, &c2, "mid") } });
        if u.menu != MenuId::Pick {
            let ok = o.panic.is_none();
            let mut r = self.base(u, c, tabname);
            r.op = "picker open".into();
            self.push(r, if ok { "DEAD" } else { "ERROR" }, "picker open", Some(&o), "the value did not open the list".into());
            return;
        }
        let j = opts.iter().position(|x| *x != cur).unwrap_or(0);
        let j = if opts.len() > 2 && opts[j] == cur { 1 } else { j };
        let pj = (opts.len() / 2).max(1).min(opts.len() - 1);
        let j = if opts[pj] != cur { pj } else { j };
        // the picker's own cell
        let rect = if u.sheet_active() { u.sheet_pick_cells().0.into_iter().find(|x| x.0 == j).map(|x| x.1) } else { u.dlg_layout().and_then(|d| d.rows.into_iter().find(|r| r.it == Item::PickOpt(j)).map(|r| r.ctrl)) };
        let Some(rect) = rect else {
            let mut r = self.base(u, c, tabname);
            r.op = "picker choose".into();
            self.push(r, "OK", "picker choose", None, format!("option {} not on the first page ({} options)", j, opts.len()));
            u.close_picker();
            return;
        };
        let want = opts[j].clone();
        let pc = C { it, ck: CK::Link, rect, opts: vec![], sel: None, tab: c.tab, sheet: c.sheet };
        let o = self.probe(u, &pc, drift, &mut |u| { let (x, y) = cen(rect); tap_at(u, x, y); });
        let shown = u.value(it);
        let back = u.menu == self.ctx.menu && u.menu_open;
        let mut r = self.base(u, c, tabname);
        let mut v = "OK";
        let mut ev = format!("'{}' -> '{}' (wanted '{}'), picker closed {}", cur, shown, want, back);
        if let Some((vv, e)) = self.verdict_common(&o) { v = vv; ev = e; }
        else if shown != want && !shown.eq_ignore_ascii_case(&want) { v = if o.mf.is_empty() { "DEAD" } else { "STALE-LABEL" }; }
        else if !back { v = "NO-RETURN"; }
        else if !target_fits(&format!("{:?}", it), &r.label, &o.mf) && !o.mf.is_empty() { v = "WRONG-TARGET"; }
        r.op = "picker choose".into();
        self.push(r, v, "picker choose", Some(&o), ev);
        // restore
        if u.menu == MenuId::Pick { u.close_picker(); }
        self.recover(u, c.tab, &mut None);
        if u.value(it) != cur {
            let mut guard = 0;
            while u.value(it) != cur && guard < opts.len() + 2 { guard += 1; u.adjust(it, 1); }
        }
    }

    /// Back to the page under test; a closed menu opens again.
    fn recover(&mut self, u: &mut Ui, tab: usize, _x: &mut Option<()>) {
        if u.menu == MenuId::Pick { u.close_picker(); }
        u.numpad = None;
        u.dialog = None;
        u.power_dlg = false;
        u.sysinfo = None;
        u.wlan.osk = None;
        if u.m.layout != self.ctx.layout { u.m.layout = self.ctx.layout; }
        reopen(u, &self.ctx, tab);
    }

    fn walk_menu(&mut self, pages_note: &mut Vec<String>) {
        let mut u = fresh_for(&mut self.ctx);
        let spec_tabs: Vec<&'static str> = u.sheet_spec(self.ctx.menu).map(|s| s.tabs.iter().map(|t| t.name).collect()).unwrap_or_default();
        let ntabs = tab_rects(&u).len().max(1);
        for tab in 0..ntabs {
            if u.m.layout != self.ctx.layout || !u.menu_open || u.menu != self.ctx.menu { u = fresh_for(&mut self.ctx); }
            reopen(&mut u, &self.ctx, tab);
            let tabname = spec_tabs.get(tab).copied().unwrap_or("").to_string();
            let cs = controls(&u, tab);
            if cs.is_empty() {
                pages_note.push(format!("{} | {:?} | {} | no controls enumerated (custom page or empty)", lname(self.ctx.layout), self.ctx.menu, self.ctx.variant));
                continue;
            }
            let mut since = 0;
            for c in cs {
                since += 1;
                let res = catch_unwind(AssertUnwindSafe(|| {
                    let c1 = find(&u, &c).unwrap_or_else(|| c.clone());
                    self.test_control(&mut u, &c1, &tabname);
                }));
                if res.is_err() {
                    let r = self.base(&u, &c, &tabname);
                    self.push(r, "ERROR", "harness", None, "panic while testing".into());
                    u = fresh_for(&mut self.ctx);
                    reopen(&mut u, &self.ctx, tab);
                } else if since % 20 == 0 || u.m.layout != self.ctx.layout {
                    u = fresh_for(&mut self.ctx);
                    reopen(&mut u, &self.ctx, tab);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ the bars

/// The top and bottom bar buttons: each one is pressed and must do something.
fn walk_bars(w: &mut Walker) {
    let l = w.ctx.layout;
    let mut names: Vec<(String, Rect)> = Vec::new();
    {
        let u = build(l, 7000);
        for (b, label, r) in u.top_buttons() { names.push((format!("top:{}:{:?}", label, b), r)); }
        names.push(("top:run pill".into(), u.run_pill()));
        names.push(("top:Single".into(), u.single_btn()));
        names.push(("bottom:Menu".into(), u.menu_btn()));
        let (chs, tr, trg) = u.cards();
        for (i, r) in chs.iter().enumerate() { names.push((format!("bottom:CH{} card", i + 1), *r)); }
        names.push(("bottom:Time card".into(), tr));
        names.push(("bottom:Trigger card".into(), trg));
    }
    for (name, _) in names.iter() {
        let name = name.clone();
        let mut u = build(l, 7001);
        let rect = {
            let mut found = None;
            for (b, label, r) in u.top_buttons() { if format!("top:{}:{:?}", label, b) == name { found = Some(r); } }
            if found.is_none() {
                let (chs, tr, trg) = u.cards();
                found = match name.as_str() {
                    "top:run pill" => Some(u.run_pill()), "top:Single" => Some(u.single_btn()), "bottom:Menu" => Some(u.menu_btn()),
                    "bottom:Time card" => Some(tr), "bottom:Trigger card" => Some(trg),
                    n if n.starts_with("bottom:CH") => { let i: usize = n[9..10].parse::<usize>().unwrap_or(1) - 1; Some(chs[i]) }
                    _ => None,
                };
            }
            found.unwrap()
        };
        if rect.w <= 0 || rect.h <= 0 { continue; }
        let dummy = C { it: Item::Open(MenuId::Main), ck: CK::Link, rect, opts: vec![], sel: None, tab: 0, sheet: false };
        let skip = name.contains("System") && false;
        let _ = skip;
        let mut row = Row { layout: lname(l).into(), menu: "bar".into(), variant: String::new(), tab: String::new(), label: name.clone(), item: name.clone(), kind: "Bar".into(), op: "tap".into(), verdict: "OK".into(), fields: String::new(), frame: String::new(), evidence: String::new(), scpi: String::new(), note: String::new() };
        let drift = { let mut u2 = build(l, 7002); w.drift(&mut u2, &dummy) };
        let o = w.probe(&mut u, &dummy, &drift, &mut |u| { let (x, y) = cen(rect); tap_at(u, x, y); });
        row.fields = o.fields();
        row.frame = o.frame_s();
        if !o.toast.is_empty() { row.note = format!("toast '{}'", cut(&o.toast)); }
        if let Some((v, e)) = w.verdict_common(&o) { row.verdict = v.into(); row.evidence = e; }
        else if !o.observable() { row.verdict = "DEAD".into(); row.evidence = "nothing changed".into(); }
        else { row.evidence = format!("menu now {:?}/{}", u.menu, u.menu_open); }
        w.rows.push(row);
    }
}

// ------------------------------------------------------------------------------------------------ the plan

fn plan(l: Layout) -> Vec<(MenuId, String, Vec<(Item, usize)>)> {
    let mut v: Vec<(MenuId, String, Vec<(Item, usize)>)> = Vec::new();
    let base = [MenuId::Main, MenuId::Horizontal, MenuId::Trigger, MenuId::Acquire, MenuId::Measure, MenuId::MeasAdd, MenuId::MeasHold, MenuId::MeasPair, MenuId::MeasStat,
        MenuId::MeasLevels, MenuId::MeasFall, MenuId::Dvm, MenuId::Cursor, MenuId::Display, MenuId::Utility, MenuId::Math, MenuId::Ref, MenuId::Decode, MenuId::Bus,
        MenuId::Storage, MenuId::Mask, MenuId::Zone, MenuId::Spectrum, MenuId::Eye, MenuId::Power, MenuId::Magnetic, MenuId::Inverter, MenuId::Dpt, MenuId::Identify,
        MenuId::SelfCal, MenuId::Network, MenuId::Remote, MenuId::Time, MenuId::Bode, MenuId::Sim, MenuId::SimAdvanced, MenuId::Afg, MenuId::BwTest, MenuId::ViewMore,
        MenuId::La, MenuId::LaLines, MenuId::LaPat, MenuId::Windows, MenuId::Search, MenuId::Logger, MenuId::MeasLegacy, MenuId::Channel(0), MenuId::Channel(1),
        MenuId::Channel(3), MenuId::ChMore(0), MenuId::ChMore(2)];
    for m in base { v.push((m, "base".into(), vec![])); }
    // variants: a menu whose rows depend on a choice is walked once per choice
    for k in 1..4 { v.push((MenuId::Cursor, format!("CurMode={k}"), vec![(Item::CurMode, k)])); }
    for k in [1usize, 2, 4, 6] { v.push((MenuId::Math, format!("MathOpI={k}"), vec![(Item::MathOpI, k)])); }
    for k in 1..6 { v.push((MenuId::Trigger, format!("TrigType={k}"), vec![(Item::TrigType, k)])); }
    for k in 1..3 { v.push((MenuId::Power, format!("PowMode={k}"), vec![(Item::PowMode, k)])); }
    v.push((MenuId::Decode, "DecOn".into(), vec![(Item::DecOn, 1)]));
    v.push((MenuId::Spectrum, "SpecOn".into(), vec![(Item::SpecOn, 1)]));
    v.push((MenuId::Eye, "EyeOn".into(), vec![(Item::EyeOn, 1)]));
    v.push((MenuId::Mask, "MaskOn".into(), vec![(Item::MaskOn, 1)]));
    v.push((MenuId::Display, "pane=trend".into(), vec![]));
    v.push((MenuId::Ref, "ref-stored".into(), vec![]));
    let _ = l;
    v
}

fn walk_layout(l: Layout) {
    let scpi_map = load_scpi_map();
    let mut w = Walker { ctx: Ctx { layout: l, menu: MenuId::Main, variant: String::new(), pre: vec![], scpi_map, ui_n: 0 }, rows: Vec::new(), skipped: Vec::new() };
    let mut notes: Vec<String> = Vec::new();
    let _ = std::fs::create_dir_all(OUT_DIR);
    let flush = |w: &Walker, notes: &Vec<String>| {
        let body = w.rows.iter().map(|r| r.tsv()).collect::<Vec<_>>().join("\n");
        let _ = std::fs::write(std::path::Path::new(OUT_DIR).join(format!("results-{}{}.tsv", lname(l), std::env::var("FUNC_ONLY").map(|o| format!("-{o}")).unwrap_or_default())), body);
        let _ = std::fs::write(std::path::Path::new(OUT_DIR).join(format!("notes-{}{}.txt", lname(l), std::env::var("FUNC_ONLY").map(|o| format!("-{o}")).unwrap_or_default())), format!("{}\n--- skipped\n{}", notes.join("\n"), w.skipped.join("\n")));
    };
    let only = std::env::var("FUNC_ONLY").ok();
    if only.as_deref().map_or(true, |o| o == "bars") { walk_bars(&mut w); flush(&w, &notes); }
    for (menu, variant, pre) in plan(l) {
        if let Some(o) = &only { if o != "bars" && !format!("{:?}", menu).contains(o.as_str()) { continue; } if o == "bars" { continue; } }
        if std::env::var("FUNC_SKIP").is_ok_and(|s| s.split(',').any(|x| x == format!("{:?}", menu))) { continue; }
        w.ctx.menu = menu;
        w.ctx.variant = variant;
        w.ctx.pre = pre;
        let t = std::time::Instant::now();
        let r = catch_unwind(AssertUnwindSafe(|| w.walk_menu(&mut notes)));
        if r.is_err() { notes.push(format!("{:?} {}: walk panicked", menu, w.ctx.variant)); }
        println!("FC {} {:?} {} {} rows {:.1}s", lname(l), menu, w.ctx.variant, w.rows.len(), t.elapsed().as_secs_f32());
        flush(&w, &notes);
    }
    flush(&w, &notes);
}

#[test]
#[ignore]
fn funccheck_minimal() { walk_layout(Layout::Minimal); }

#[test]
#[ignore]
fn funccheck_default() { walk_layout(Layout::Default); }

#[test]
#[ignore]
fn funccheck_studio() { walk_layout(Layout::Studio); }

// ------------------------------------------------------------------------------------------------ display effects (acquisition running)

/// Coloured ink of the plot area (the traces and the colour grading, not the grey grid), lit pixels, distinct colours,
/// grey ink (the grid).
fn plot_metrics(u: &mut Ui, n: usize) -> (u64, usize, usize, u64) {
    for _ in 0..n { frame(u); }
    u.toast = None;
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    let g = u.grid;
    let (mut ink, mut lit, mut grey) = (0u64, 0usize, 0u64);
    let mut cols = std::collections::HashSet::new();
    for y in g.y..g.y + g.h {
        for x in g.x..g.x + g.w {
            let p = cv.px[(y * cv.w + x) as usize];
            let (r, gr, b) = ((p >> 16) & 255, (p >> 8) & 255, p & 255);
            let (mx, mn) = (r.max(gr).max(b), r.min(gr).min(b));
            if mx - mn > 80 { ink += mx as u64; lit += 1; } else if mx > 30 { grey += mx as u64; }
            cols.insert(p & 0xffffff);
        }
    }
    (ink, lit, cols.len(), grey)
}

/// Each Display option applied to a running acquisition: does the plot change the way the label says?
/// `adj-N` steps a value N times (a - and + press), anything else selects the option by its name.
#[test]
#[ignore]
fn display_effects() {
    let mut out: Vec<String> = Vec::new();
    for l in [Layout::Minimal, Layout::Default, Layout::Studio] {
        let cases: Vec<(Item, &str, &str, Vec<(Item, &str)>)> = vec![
            (Item::Persist, "1 s", "coloured ink grows", vec![]),
            (Item::Persist, "Infinite", "coloured ink grows", vec![]),
            (Item::ColorGrade, "On", "colours change", vec![]),
            (Item::ColorGrade, "On", "colours change (persistence Infinite)", vec![(Item::Persist, "Infinite")]),
            (Item::Intensity, "adj+8", "coloured ink grows", vec![]),
            (Item::Intensity, "adj-10", "coloured ink drops", vec![]),
            (Item::GridStyle, "Light", "grey ink drops", vec![]),
            (Item::GridStyle, "None", "grey ink drops", vec![]),
            (Item::GridBright, "adj-9", "grey ink drops", vec![]),
            (Item::GridFrame, "Off", "grey ink drops", vec![]),
            (Item::Dots, "On", "trace changes", vec![]),
            (Item::ZInput, "CH2", "trace brightness changes (CH2 on)", vec![]),
            (Item::Histogram, "Vertical", "histogram drawn", vec![]),
            (Item::Histogram, "Horizontal", "histogram drawn", vec![]),
            (Item::XyMode, "XY", "XY plot", vec![]),
            (Item::Hdmi, "Mirror", "HDMI output only", vec![]),
            (Item::Backlight, "adj-8", "panel LED only", vec![]),
            (Item::TextSize, "Large", "labels grow", vec![]),
        ];
        for (it, want, what, pre) in cases {
            let mut u = build(l, 8000);
            u.m.ch[1].on = true;
            for (pi, pw) in &pre { u.select_option(*pi, pw); }
            u.m.run = RunState::Run;
            let base = plot_metrics(&mut u, 30);
            let base2 = plot_metrics(&mut u, 6);
            let cur = u.value(it);
            if let Some(n) = want.strip_prefix("adj") { let n: i32 = n.parse().unwrap_or(0); for _ in 0..n.abs() { u.adjust(it, n.signum()); } } else { u.select_option(it, want); }
            let after = plot_metrics(&mut u, 30);
            let nv = u.value(it);
            let rel = |a: u64, b: u64| (b as f64 - a as f64) / (a.max(1) as f64) * 100.0;
            let relu = |a: usize, b: usize| (b as f64 - a as f64) / (a.max(1) as f64) * 100.0;
            out.push(format!("{}\t{:?}\t{} -> {}\t{}\tink {:+.1}% (noise {:.1}%)\tgrey {:+.1}% (noise {:.1}%)\tlit {} -> {} ({:+.1}%)\tcolours {} -> {}", lname(l), it, cur, nv, what,
                rel(base.0, after.0), rel(base.0, base2.0).abs(), rel(base.3, after.3), rel(base.3, base2.3).abs(), base.1, after.1, relu(base.1, after.1), base.2, after.2));
        }
    }
    let _ = std::fs::write(std::path::Path::new(OUT_DIR).join("results-effects.tsv"), out.join("\n"));
    for o in &out { println!("FC {o}"); }
}

/// Studio, CH1 > More: the Unit row, pressed by hand (the walk saw no change).
#[test]
#[ignore]
fn studio_unit_probe() {
    let mut u = build(Layout::Studio, 9000);
    u.open_menu(MenuId::Channel(0));
    frame(&mut u);
    let tabs = tab_rects(&u);
    println!("FC tabs {:?}", tabs);
    for t in 0..tabs.len() {
        let (x, y) = cen(tabs[t]);
        tap_at(&mut u, x, y);
        let cs = controls(&u, t);
        println!("FC tab {} controls {:?}", t, cs.iter().map(|c| format!("{:?}:{:?}", c.it, c.ck)).collect::<Vec<_>>());
        for c in cs.iter().filter(|c| matches!(c.it, Item::ChUnit(_) | Item::ChSkew(_) | Item::ChExpand)) {
            let v0 = u.value(c.it);
            println!("FC {:?} rect {:?} opts {:?} sel {:?} value {}", c.it, c.rect, c.opts, c.sel, v0);
            let part = if c.ck == CK::Seg { "seg1" } else { "minus" };
            tap_c(&mut u, c, part);
            println!("FC  after {} -> {} menu {:?}", part, u.value(c.it), u.menu);
        }
    }
}

/// Utility rows that showed no reaction in the walk: by tap and by `activate`.
#[test]
#[ignore]
fn utility_probe() {
    for l in [Layout::Minimal, Layout::Studio] {
        for it in [Item::Open(MenuId::Network), Item::Open(MenuId::Time), Item::Open(MenuId::SelfCal), Item::SysInfo, Item::About, Item::Shot, Item::Graphics] {
            for by_tap in [true, false] {
                let mut u = build(l, 9100);
                u.open_menu(MenuId::Utility);
                frame(&mut u);
                let c = controls(&u, 0).into_iter().find(|c| c.it == it);
                let Some(c) = c else { println!("FC {:?} {:?}: not on the page", l, it); continue };
                if by_tap { tap_c(&mut u, &c, "tap"); } else { u.activate(it); frame(&mut u); }
                let t = u.toast.as_ref().map(|t| t.0.clone());
                println!("FC {:?} {:?} tap={} rect {:?} -> menu {:?}/{} dialog {} toast {:?} shot {:?}", l, it, by_tap, c.rect, u.menu, u.menu_open, u.dialog.is_some() || u.sysinfo.is_some(), t, u.shot_request);
            }
        }
    }
}

/// Studio sheet rows that showed no reaction in the walk, pressed one at a time on a fresh Ui.
#[test]
#[ignore]
fn studio_row_probe() {
    let cases: Vec<(MenuId, Vec<(Item, usize)>, Item, &str)> = vec![
        (MenuId::Math, vec![(Item::MathOpI, 1)], Item::MathA, "seg1"),
        (MenuId::Math, vec![(Item::MathOpI, 1)], Item::MathB, "seg0"),
        (MenuId::Trigger, vec![(Item::TrigType, 4)], Item::PatLevel, "minus"),
        (MenuId::Trigger, vec![(Item::TrigType, 3)], Item::TrigLevel2, "minus"),
        (MenuId::Trigger, vec![(Item::TrigType, 4)], Item::TrigSrc, "seg0"),
    ];
    for (menu, pre, it, part) in cases {
        let mut ctx = Ctx { layout: Layout::Studio, menu, variant: "probe".into(), pre, scpi_map: vec![], ui_n: 9200 };
        let mut u = fresh_for(&mut ctx);
        let ntabs = tab_rects(&u).len().max(1);
        let mut done = false;
        for tab in 0..ntabs {
            reopen(&mut u, &ctx, tab);
            let cs = controls(&u, tab);
            if let Some(c) = cs.iter().find(|c| c.it == it) {
                let v0 = u.value(it);
                println!("FC {:?} {:?} tab {} ck {:?} opts {:?} rect {:?} before '{}'", menu, it, tab, c.ck, c.opts, c.rect, v0);
                tap_c(&mut u, c, part);
                println!("FC   {} -> '{}' menu {:?}", part, u.value(it), u.menu);
                done = true;
            }
        }
        if !done { println!("FC {:?} {:?}: not on any tab; items {:?}", menu, it, u.items(menu)); }
    }
}
