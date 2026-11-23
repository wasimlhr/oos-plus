// Copyright (C) 2026 OOS contributors
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:TRIGger:RS232|SPI|IIC:*`: the protocol trigger's settings under the stock command names, and fork
//! additions for what the stock commands leave out (relations, limits, masks):
//! `:RELation EQ|NE|GT|LT|IN|OUT`, `:DMAX <n>`, `:MASK <n>`, `:ARELation`, `:AMAX` (docs/fork/serial-ui.md).

use super::{kw, num, num64};
use crate::model::*;

/// A number, decimal or 0x hex.
fn int(s: &str) -> Option<u64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => num64(s).filter(|x| *x >= 0.0).map(|x| x as u64),
    }
}

/// A setting of keywords: each row is the word the query answers, the long form a command takes too, and the value.
/// The query answers the word of `cur`; a command (`a` in capitals) sets the value of the row that names it, None for
/// a word in no row.
fn keyword<T: Copy + PartialEq>(
    cur: &mut T,
    rows: &[(&str, &str, T)],
    a: &str,
    query: bool,
) -> Option<Option<Vec<u8>>> {
    if query {
        return rows.iter().find(|r| r.2 == *cur).map(|r| Some(r.0.as_bytes().to_vec()));
    }
    *cur = rows.iter().find(|r| a == r.0 || a == r.1)?.2;
    Some(None)
}

const RELS: [(&str, &str, Rel); 6] = [
    ("EQ", "EQUAL", Rel::Eq),
    ("NE", "NEQUAL", Rel::Ne),
    ("GT", "GREATER", Rel::Gt),
    ("LT", "LESS", Rel::Lt),
    ("IN", "IN", Rel::In),
    ("OUT", "OUT", Rel::Out),
];

/// One `:TRIGger:<bus>:<key>` command or query on `m`: `Some(Some(answer))` for a query, `Some(None)` for a setting
/// taken, None for an unknown key or a bad value.
pub fn trig(m: &mut Model, bus: &str, key: &str, arg: &str, query: bool) -> Option<Option<Vec<u8>>> {
    let t = |s: String| Some(Some(s.into_bytes()));
    let a = arg.trim().to_ascii_uppercase();
    // value, relation, limits: the same for every bus
    if kw(key, "DATA") || (kw(bus, "IIC") && kw(key, "ADDRess")) {
        if query {
            return t(m.trig_bus_val.to_string());
        }
        m.trig_bus_val = int(arg)?.min(crate::ui::BUS_VAL_MAX);
        return Some(None);
    }
    if kw(key, "RELation") || kw(key, "ARELation") {
        let rel = if kw(key, "ARELation") { &mut m.ser.addr_rel } else { &mut m.ser.rel };
        return keyword(rel, &RELS, &a, query);
    }
    if kw(key, "DMAX") || kw(key, "AMAX") {
        if kw(key, "AMAX") {
            if query {
                return t(m.ser.addr_max.to_string());
            }
            m.ser.addr_max = int(arg)?.min(0x3ff) as u32;
        } else {
            if query {
                return t(m.ser.max.to_string());
            }
            m.ser.max = int(arg)?.min(crate::ui::BUS_VAL_MAX);
        }
        return Some(None);
    }
    if kw(key, "MASK") {
        if query {
            return t((m.ser.mask & crate::ui::BUS_VAL_MAX).to_string());
        }
        m.ser.mask = int(arg)?;
        return Some(None);
    }
    if kw(bus, "RS232") {
        return rs232(m, key, arg, &a, query);
    }
    if kw(bus, "SPI") {
        return spi(m, key, arg, &a, query);
    }
    if kw(bus, "IIC") {
        return iic(m, key, arg, &a, query);
    }
    None
}

/// `:TRIGger:RS232:<key>` on `m`; `a` is `arg` in capitals.
fn rs232(m: &mut Model, key: &str, arg: &str, a: &str, query: bool) -> Option<Option<Vec<u8>>> {
    let t = |s: String| Some(Some(s.into_bytes()));
    if kw(key, "WHEN") {
        if query {
            return t(match (m.trig_bus, m.ser.uart_err) {
                (BusWhat::Data, _) => "DATA",
                (BusWhat::Error, UartErr::Parity) => "CERR",
                (BusWhat::Error, UartErr::Framing) => "ERR",
                (BusWhat::Error, UartErr::Either) => "EITH",
                _ => "STAR",
            }
            .into());
        }
        (m.trig_bus, m.ser.uart_err) = match a {
            "STAR" | "START" => (BusWhat::Frame, m.ser.uart_err),
            "DATA" => (BusWhat::Data, m.ser.uart_err),
            "ERR" | "ERROR" => (BusWhat::Error, UartErr::Framing),
            "CERR" | "CERROR" => (BusWhat::Error, UartErr::Parity),
            "EITH" | "EITHER" => (BusWhat::Error, UartErr::Either),
            _ => return None,
        };
    } else if kw(key, "POLarity") {
        return keyword(&mut m.ser.uart_inv, &[("POS", "POSITIVE", false), ("NEG", "NEGATIVE", true)], a, query);
    } else if kw(key, "STOP") {
        let rows = [("1", "1", StopBits::One), ("1.5", "1.5", StopBits::OneHalf), ("2", "2", StopBits::Two)];
        return keyword(&mut m.ser.uart_stop, &rows, a, query);
    } else if kw(key, "PARity") {
        use oos_dsp::decode::Parity;
        let rows = [("NONE", "NONE", Parity::None), ("EVEN", "EVEN", Parity::Even), ("ODD", "ODD", Parity::Odd)];
        return keyword(&mut m.dec_parity, &rows, a, query);
    } else if kw(key, "WIDTh") {
        if query {
            return t(m.dec_bits.to_string());
        }
        m.dec_bits = int(arg).filter(|w| (5..=8).contains(w))? as u32;
    } else if kw(key, "BAUD") {
        let b = &crate::decode::BAUDS;
        if query {
            return t(format!("{}", b[m.dec_baud]));
        }
        let want = int(arg)? as f64;
        m.dec_baud = (0..b.len()).min_by(|&i, &j| (b[i] - want).abs().total_cmp(&(b[j] - want).abs()))?;
    } else {
        return None;
    }
    Some(None)
}

/// `:TRIGger:SPI:<key>` on `m`; `a` is `arg` in capitals.
fn spi(m: &mut Model, key: &str, arg: &str, a: &str, query: bool) -> Option<Option<Vec<u8>>> {
    let t = |s: String| Some(Some(s.into_bytes()));
    if kw(key, "WHEN") {
        if query {
            return t(if m.dec_cs.is_some() { "CS" } else { "TIM" }.into());
        }
        match a {
            // chip select: the decoder's CS line, CH3 when it has none
            "CS" => m.dec_cs = m.dec_cs.or(Some(2)),
            "TIM" | "TIMEOUT" => {
                m.dec_cs = None;
                if m.ser.spi_timeout <= 0.0 {
                    m.ser.spi_timeout = 1e-6;
                }
            }
            _ => return None,
        }
    } else if kw(key, "TIMeout") {
        if query {
            return t(format!("{:e}", m.ser.spi_timeout));
        }
        m.ser.spi_timeout = num(arg).filter(|x| *x >= 0.0 && *x <= 1.0)?;
    } else if kw(key, "SLOPe") {
        if query {
            return t(if matches!(m.dec_mode, 0 | 3) { "POS" } else { "NEG" }.into());
        }
        m.dec_mode = match a {
            "POS" | "POSITIVE" => 0,
            "NEG" | "NEGATIVE" => 1,
            _ => return None,
        };
    } else if kw(key, "MODE") {
        return keyword(&mut m.ser.spi_cs_high, &[("HIGH", "HIGH", true), ("LOW", "LOW", false)], a, query);
    } else if kw(key, "WIDTh") {
        if query {
            return t(m.dec_bits.to_string());
        }
        m.dec_bits = int(arg).filter(|w| (4..=32).contains(w))? as u32;
    } else {
        return None;
    }
    Some(None)
}

/// `:TRIGger:IIC:<key>` on `m`; `a` is `arg` in capitals.
fn iic(m: &mut Model, key: &str, arg: &str, a: &str, query: bool) -> Option<Option<Vec<u8>>> {
    let t = |s: String| Some(Some(s.into_bytes()));
    if kw(key, "WHEN") {
        // repeated start, stop and address + data have no menu condition yet
        let rows = [
            ("STAR", "START", BusWhat::Frame),
            ("ADDR", "ADDRESS", BusWhat::Id),
            ("DATA", "DATA", BusWhat::Data),
            ("NACK", "NACKNOWLEDGE", BusWhat::Error),
        ];
        return keyword(&mut m.trig_bus, &rows, a, query);
    } else if kw(key, "AWIDth") {
        return keyword(&mut m.ser.ten_bit, &[("7", "7", false), ("10", "10", true)], a, query);
    } else if kw(key, "DIRection") {
        let rows =
            [("READ", "READ", I2cDir::Read), ("WRIT", "WRITE", I2cDir::Write), ("RWR", "RWRITE", I2cDir::Either)];
        return keyword(&mut m.ser.dir, &rows, a, query);
    } else if kw(key, "DBYTes") {
        if query {
            return t(m.ser.bytes.to_string());
        }
        m.ser.bytes = int(arg).filter(|&n| n <= 5)? as u8;
    } else {
        return None;
    }
    Some(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(m: &mut Model, bus: &str, key: &str) -> String {
        String::from_utf8(trig(m, bus, key, "", true).unwrap().unwrap()).unwrap()
    }

    fn set(m: &mut Model, bus: &str, key: &str, v: &str) -> bool {
        trig(m, bus, key, v, false) == Some(None)
    }

    #[test]
    fn rs232() {
        let mut m = Model::default();
        for (key, v, back) in [
            ("WHEN", "DATA", "DATA"),
            ("WHEN", "CERRor", "CERR"),
            ("WHEN", "ERR", "ERR"),
            ("POLarity", "NEGative", "NEG"),
            ("STOP", "1.5", "1.5"),
            ("PARity", "EVEN", "EVEN"),
            ("WIDTh", "7", "7"),
            ("BAUD", "115200", "115200"),
            ("DATA", "65", "65"),
            ("RELation", "IN", "IN"),
            ("DMAX", "0x39", "57"),
        ] {
            assert!(set(&mut m, "RS232", key, v), "{} {}", key, v);
            assert_eq!(q(&mut m, "RS232", key), back, "{}", key);
        }
        assert!(m.ser.uart_inv && m.ser.uart_err == UartErr::Framing && m.dec_bits == 7 && m.ser.rel == Rel::In);
        assert!(
            !set(&mut m, "RS232", "WIDTh", "9")
                && !set(&mut m, "RS232", "STOP", "3")
                && !set(&mut m, "RS232", "RELation", "ABOUT")
        );
    }

    #[test]
    fn spi_and_iic() {
        let mut m = Model { dec_cs: Some(2), ..Default::default() };
        assert!(set(&mut m, "SPI", "WHEN", "TIMeout"));
        assert_eq!((m.dec_cs, m.ser.spi_timeout, q(&mut m, "SPI", "WHEN")), (None, 1e-6, "TIM".to_string()));
        for (key, v, back) in [
            ("TIMeout", "2e-6", "2e-6"),
            ("SLOPe", "NEG", "NEG"),
            ("MODE", "HIGH", "HIGH"),
            ("WIDTh", "16", "16"),
            ("MASK", "0xff00", "65280"),
            ("WHEN", "CS", "CS"),
        ] {
            assert!(set(&mut m, "SPI", key, v), "{} {}", key, v);
            assert_eq!(q(&mut m, "SPI", key), back, "{}", key);
        }
        assert_eq!((m.dec_mode, m.dec_cs, m.ser.spi_cs_high), (1, Some(2), true));
        for (key, v, back) in [
            ("WHEN", "ADDRess", "ADDR"),
            ("AWIDth", "10", "10"),
            ("ADDRess", "0x2a5", "677"),
            ("ARELation", "GT", "GT"),
            ("AMAX", "1023", "1023"),
            ("DIRection", "READ", "READ"),
            ("WHEN", "DATA", "DATA"),
            ("DBYTes", "3", "3"),
            ("DATA", "0x123456", "1193046"),
        ] {
            assert!(set(&mut m, "IIC", key, v), "{} {}", key, v);
            assert_eq!(q(&mut m, "IIC", key), back, "{}", key);
        }
        assert!(
            !set(&mut m, "IIC", "WHEN", "RESTart")
                && !set(&mut m, "IIC", "AWIDth", "8")
                && !set(&mut m, "IIC", "DBYTes", "6")
        );
    }
}
