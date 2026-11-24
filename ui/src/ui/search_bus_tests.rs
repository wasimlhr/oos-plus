// SPDX-License-Identifier: GPL-2.0-or-later

//! A bus search: the packets of the active decoder that the bus trigger's condition (`ModuleInfo::trigger_match`)
//! accepts, on UART bytes made by the dsp generator.

use super::addrail::SearchSpec;
use super::*;

const BYTES: [u8; 4] = [0x55, 0xa5, 0x55, 0x00];

/// A reference that holds `BYTES` as UART at 115200 Bd (even parity), decoded by decoder 1 with `parity`, and a
/// Studio search of the bus type.
fn uart_ui(parity: oos_dsp::decode::Parity) -> Ui {
    let baud = 115_200.0;
    let dt = 1.0 / (baud * 20.0);
    let v = oos_dsp::sim::uart_logic(&BYTES, baud, 8, Some(true), 0.0, 0.0, 3.3).sample(dt, 20.0 / baud);
    let mut u = crate::ui::testkit::demo_ui();
    u.m.layout = Layout::Studio;
    u.refs[0] = Some(RefWave { label: "bus".into(), vpd: 0.5, ..RefWave::test(v, dt, 0.0) });
    u.m.dec_proto = Proto::UART;
    u.m.dec_on = true;
    u.m.dec_src = DEC_REF_BASE;
    u.m.dec_baud = crate::decode::BAUDS.iter().position(|&b| b == baud).unwrap();
    u.m.dec_thr = 1.65;
    u.m.dec_bits = 8;
    u.m.dec_parity = parity;
    u.m.run = RunState::Stop;
    u.studio_search_add(Some(WaveId::Ch(0)));
    u.select_option(Item::SrchType, "Bus");
    u
}

fn set(u: &mut Ui, f: impl FnOnce(&mut SearchSpec)) -> usize {
    let mut sp = u.studio.search().unwrap();
    f(&mut sp);
    u.studio.search_put(sp);
    u.studio_search_events().len()
}

#[test]
fn a_bus_search_marks_the_decoded_data_of_the_active_decoder() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    assert_eq!(u.studio.search().unwrap().kind, TrigType::Bus);
    let packets = u.decode_packets().unwrap();
    let data: Vec<_> = packets.iter().filter(|p| p.kind == oos_dsp::decode::Kind::Data).collect();
    assert_eq!(data.iter().map(|p| p.value).collect::<Vec<_>>(), BYTES.map(u32::from));
    // the same condition as the bus trigger
    assert_eq!(set(&mut u, |s| (s.bus, s.val) = (BusWhat::Data, 0x55)), 2);
    let at: Vec<f64> = u.studio_search_events().to_vec();
    assert_eq!(at, vec![data[0].t1, data[2].t1], "at the end of each packet");
    assert_eq!(set(&mut u, |s| s.val = 0xa5), 1);
    assert_eq!(set(&mut u, |s| s.val = 0x12), 0);
    // a frame is any packet of the decoder: the trigger has the same condition
    let frames = set(&mut u, |s| s.bus = BusWhat::Frame);
    let mut m = u.m.clone();
    m.trig_bus = BusWhat::Frame;
    assert_eq!(frames, crate::trig::points(&m, &packets).len());
    assert!(frames >= BYTES.len());
    // and the events are the ones the trigger search finds
    u.set_trig_type(TrigType::Bus);
    u.m.trig_bus = BusWhat::Frame;
    assert_eq!(u.studio_search_events().len(), frames);
}

#[test]
fn a_bus_search_finds_errors_and_follows_the_decoder() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    assert_eq!(set(&mut u, |s| s.bus = BusWhat::Error), 0, "the parity is right");
    // the decoder expects the other parity: every byte is wrong
    u.m.dec_parity = oos_dsp::decode::Parity::Odd;
    assert_eq!(u.studio_search_events().len(), BYTES.len(), "the decoder's settings are the search's");
    // the decoder is off: nothing was decoded
    u.m.dec_on = false;
    assert!(u.studio_search_events().is_empty());
}

#[test]
fn a_bus_search_is_listed_counted_and_jumps_to_its_events() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    set(&mut u, |s| (s.bus, s.val) = (BusWhat::Data, 0x55));
    assert_eq!(u.studio_search_count(), "2 events");
    let rows = u.table_rows(tables::TableKind::Search);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].text, "Data 0x55");
    assert_eq!(rows[0].who, "D1 UART R1 #1".replace("D1 UART R1", &u.dec_tag(u.m.dec_sel)));
    let ev = u.studio_search_events();
    u.studio_search_go(1);
    assert_eq!(u.studio.search_at[0], 1);
    assert!((u.m.delay as f64 - ev[1]).abs() < 1e-3, "the view went to the event");
    u.studio_search_go(1);
    assert_eq!(u.studio.search_at[0], 0, "and round again");
}

#[test]
fn the_sheet_of_a_bus_search_has_a_match_and_a_value_and_no_source() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    let rows = u.addrail_search_items();
    assert!(rows.contains(&Item::SrchBusWhat) && !rows.contains(&Item::SrchSrc) && !rows.contains(&Item::SrchLevel));
    assert!(!rows.contains(&Item::SrchBusVal), "a frame has no value");
    u.select_option(Item::SrchBusWhat, "Data");
    assert!(u.addrail_search_items().contains(&Item::SrchBusVal));
    assert_eq!((u.item_label(Item::SrchBusWhat), u.item_label(Item::SrchBusVal)), ("Match", "Value"));
    assert_eq!(u.options(Item::SrchBusWhat), vec!["Frame", "ID/Addr", "Data", "Error"]);
    u.set_hex(Item::SrchBusVal, 0xa5);
    assert_eq!((u.value(Item::SrchBusVal), u.value(Item::SrchBusWhat)), ("0xA5".to_string(), "Data".to_string()));
    u.adjust(Item::SrchBusVal, 1);
    assert_eq!(u.studio.search().unwrap().val, 0xa6);
    let spec = u.sheet_spec(MenuId::Search).expect("a sheet");
    assert!(spec.tabs.len() == 2 && spec.tabs.iter().all(|t| t.rows.len() <= 7));
}

#[test]
fn a_bus_search_and_the_trigger_give_each_other_their_condition() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    u.set_trig_type(TrigType::Bus);
    u.m.trig_bus = BusWhat::Data;
    u.m.trig_bus_val = 0x55;
    u.activate(Item::SrchFromTrig);
    let sp = u.studio.search().unwrap();
    assert_eq!((sp.kind, sp.bus, sp.val), (TrigType::Bus, BusWhat::Data, 0x55));
    u.set_trig_type(TrigType::Edge);
    set(&mut u, |s| (s.bus, s.val) = (BusWhat::Id, 0x123));
    u.activate(Item::SrchToTrig);
    assert_eq!((u.m.trig_type, u.m.trig_bus, u.m.trig_bus_val), (TrigType::Bus, BusWhat::Id, 0x123));
}

#[test]
fn a_view_stores_a_bus_search() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    set(&mut u, |s| (s.bus, s.val) = (BusWhat::Data, 0x55));
    let text = u.view_ui_text();
    let word = text.split_whitespace().find(|w| w.starts_with("search=")).unwrap();
    assert!(word.contains("/Bus/") && word.ends_with("/2/85"), "{word}");
    let mut v = uart_ui(oos_dsp::decode::Parity::Even);
    v.studio_search_remove();
    v.view_ui_apply(&text);
    assert_eq!(v.studio.searches, u.studio.searches);
}

#[test]
fn scpi_sets_and_asks_a_bus_search() {
    let mut u = uart_ui(oos_dsp::decode::Parity::Even);
    let mut scpi = crate::scpi::Scpi::open(0);
    let cv = Canvas::new(W, H);
    let mut ask = |u: &mut Ui, cmd: &str| String::from_utf8(scpi.exec_line(u, &cv, cmd)).unwrap().trim().to_string();
    assert_eq!(ask(&mut u, ":SEARch:MODE?"), "BUS");
    ask(&mut u, ":SEARch:BUS:WHEN DATA");
    ask(&mut u, ":SEARch:BUS:VALue 0x55");
    assert_eq!(ask(&mut u, ":SEARch:BUS:WHEN?"), "DATA");
    assert_eq!(ask(&mut u, ":SEARch:BUS:VALue?"), "0x55");
    assert_eq!(ask(&mut u, ":SEARch:COUNt?"), "2");
    ask(&mut u, ":SEARch:BUS:VALue 0xA5");
    assert_eq!(ask(&mut u, ":SEARch:COUNt?"), "1");
    // a set under the type makes the search of that type
    ask(&mut u, ":SEARch:EDGE:SLOPe POSitive");
    assert_eq!(ask(&mut u, ":SEARch:MODE?"), "EDGE");
    ask(&mut u, ":SEARch:BUS:WHEN ERRor");
    assert_eq!(ask(&mut u, ":SEARch:MODE?"), "BUS");
    assert_eq!(ask(&mut u, ":SEARch:COUNt?"), "0");
}

/// Two FlexRay frames (ID 5 and ID 9) from the dsp generator, at 10 Mbit/s, on a reference read by decoder 1.
fn flexray_ui() -> Ui {
    use oos_dsp::decode::gen::{flexray_bits, FrFrame};
    let frame = |id| FrFrame {
        ppi: false,
        nf: true,
        sync: false,
        startup: false,
        id,
        cycle: 1,
        payload: vec![0x11, 0x22],
        channel_b: false,
    };
    let mut bits = flexray_bits(&frame(5), 11);
    bits.extend(std::iter::repeat(true).take(40));
    bits.extend(flexray_bits(&frame(9), 11));
    let (bit, per) = (1.0 / oos_dsp::decode::FLEXRAY_RATES[0], 20);
    let dt = bit / per as f64;
    let l = oos_dsp::sim::bits_logic(&bits, bit, per, 0.0, 3.3);
    let v = oos_dsp::sim::sample(|t| l.v(t), 0.0, dt, per * (bits.len() + 60));
    let mut u = crate::ui::testkit::demo_ui();
    u.m.layout = Layout::Studio;
    u.refs[0] = Some(RefWave { label: "flexray".into(), vpd: 0.5, ..RefWave::test(v, dt, 0.0) });
    u.m = crate::store::parse_setup("dec_on = true\ndec_proto = FlexRay\n").unwrap_or(u.m);
    u.m.layout = Layout::Studio;
    u.m.dec_src = DEC_REF_BASE;
    u.m.dec_thr = 1.65;
    u.m.run = RunState::Stop;
    u.studio_search_add(Some(WaveId::Ch(0)));
    u.select_option(Item::SrchType, "Bus");
    u
}

#[test]
fn a_bus_search_finds_the_flexray_frames_and_ids_of_the_decoder() {
    let mut u = flexray_ui();
    let packets = u.decode_packets().unwrap();
    let is_id = |p: &&oos_dsp::decode::Packet| matches!(p.kind, oos_dsp::decode::Kind::Id { .. });
    let ids: Vec<u32> = packets.iter().filter(is_id).map(|p| p.value).collect();
    assert_eq!(ids, vec![5, 9], "{packets:?}");
    // Frame is the match of the trigger that finds a TSS
    let mut m = u.m.clone();
    m.trig_bus = BusWhat::Frame;
    let by_trigger = crate::trig::points(&m, &packets);
    assert_eq!(by_trigger.len(), 2, "the bus trigger's frames: {packets:?}");
    assert_eq!(set(&mut u, |s| s.bus = BusWhat::Frame), 2);
    let mut at = u.studio_search_events().to_vec();
    at.sort_by(f64::total_cmp);
    assert_eq!(at, by_trigger);
    assert_eq!(set(&mut u, |s| (s.bus, s.val) = (BusWhat::Id, 9)), 1);
    assert_eq!(set(&mut u, |s| s.val = 7), 0);
    assert_eq!(set(&mut u, |s| s.bus = BusWhat::Error), 0);
}
