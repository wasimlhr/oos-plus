// Copyright (C) 2026 dekajoule
// Copyright (C) 2026 OOS contributors
// SPDX-License-Identifier: GPL-2.0-or-later

//! The generator presets of the Bode plot (ui/bode.rs) and the models of the Ext generator page (`crate::extgen`): one
//! table, so a setup keeps the index of one (`bode`, its seventh value, and `ext_model`). Its first six entries are
//! upstream's Bode presets in upstream's order; OWON DGE is the seventh (oos-plus). A stored index means the same
//! generator in both builds.

/// A generator model. The commands may hold several lines (`\n`), each sent as a command of its own; `{f}` is the
/// frequency in Hz, `{a}` the amplitude in Vpp, `{c}` the channel (1 or 2), `{z}` the load (`INFinity` or `50`).
pub struct Gen {
    pub name: &'static str,
    /// sent once at the start, before the first frequency
    pub setup: &'static str,
    /// sent for every frequency
    pub step: &'static str,
    /// sent once after the first frequency
    pub on: &'static str,
    /// switches the output off again (the page's Output row)
    pub off: &'static str,
    /// the query of the error queue, sent after every command; empty: the generator is not asked
    pub err: &'static str,
    /// how the `*IDN?` answer of this model starts, in capitals; empty: Connect never picks it
    pub idn: &'static str,
    /// pause after every command, in ms
    pub pace_ms: u64,
}

const fn gen(name: &'static str, setup: &'static str, step: &'static str, on: &'static str, off: &'static str) -> Gen {
    Gen { name, setup, step, on, off, err: "", idn: "", pace_ms: 0 }
}

/// The models. Entry `BUILTIN` is the scope's own generator, entry `FREE` a generator that sweeps on its own (no
/// commands). The setup file keeps the index: new models go at the end.
///
/// CHANnel (UTG900 series generators, UTG932, UTG962): commands from the "UTG900 Series Programmable Signal Source
/// Programming Manual" REV 00, 2022-07-15 (Chinese, instrument software V3.06.004), the same in the English "UTG900E
/// Series Programming Manual" of 2022-07-15: `:CHANnel<n>:MODe CONTINUE`, `:CHANnel<n>:BASE:WAVe SINe`, `:CHANnel<n>:AMPLitude:UNIT VPP`,
/// `:CHANnel<n>:BASE:AMPLitude`, `:CHANnel<n>:BASE:OFFSet`, `:CHANnel<n>:BASE:FREQuency` (Hz), `:CHANnel<n>:OUTPut ON`;
/// every command ends with a newline. Not yet tried with a real UTG900.
///
/// OWON DGE (DGE2000 and DGE3000 series): headers from the "DGE2000/3000 Dual-Channel Arbitrary Waveform Generator
/// Programmer Manual" V1.0.0 of March 2023. Known from a DGE2070 with firmware V1.0.4.7.0 driven from a computer over
/// USBTMC: the long forms below are accepted, a value out of range is refused with an entry in the error queue and
/// the old value stays, `SYSTem:ERRor?` reads and removes the oldest entry, `*ESR?` is never answered, and the
/// amplitude is the one at the load the generator was told (`OUTPut<n>:IMPedance`). Not yet verified on hardware:
/// the generator on the USB-A port of the scope.
///
/// The `off` commands of APPL, BSWV, SCPI and CHANnel are the `on` commands with `OFF`: assumed, not yet tried.
pub const GENERATORS: [Gen; 7] = [
    gen("Built-in", "", "", "", ""),
    gen("APPL", "", ":SOUR{c}:APPL:SIN {f},{a},0", ":OUTP{c} ON", ":OUTP{c} OFF"),
    gen("BSWV", "", "C{c}:BSWV WVTP,SINE,FRQ,{f},AMP,{a},OFST,0", "C{c}:OUTP ON", "C{c}:OUTP OFF"),
    gen("SCPI", "", "FREQ {f};VOLT {a}", "OUTP ON", "OUTP OFF"),
    gen("Free-running sweep", "", "", "", ""),
    gen(
        "CHANnel",
        ":CHANnel{c}:MODe CONTINUE\n:CHANnel{c}:BASE:WAVe SINe\n:CHANnel{c}:AMPLitude:UNIT VPP\n:CHANnel{c}:BASE:AMPLitude {a}\n:CHANnel{c}:BASE:OFFSet 0",
        ":CHANnel{c}:BASE:FREQuency {f}",
        ":CHANnel{c}:OUTPut ON",
        ":CHANnel{c}:OUTPut OFF",
    ),
    Gen {
        name: "OWON DGE",
        // the load before the amplitude (the amplitude is the one at that load), the shape before the frequency (a
        // change of shape may move the frequency)
        setup: "OUTPut{c}:IMPedance {z}\nSOURce{c}:FUNCtion:SHAPe SIN\nSOURce{c}:VOLTage:LEVel:IMMediate:OFFSet 0mV\nSOURce{c}:VOLTage:LEVel:IMMediate:AMPLitude {a}Vpp",
        step: "SOURce{c}:FREQuency:FIXed {f}Hz",
        on: "OUTPut{c}:STATe ON",
        off: "OUTPut{c}:STATe OFF",
        err: "SYSTem:ERRor?",
        idn: "OWON,DGE",
        pace_ms: 20,
    },
];

pub const BUILTIN: usize = 0;
pub const FREE: usize = 4;
/// The model the Ext generator page starts with.
pub const FIRST_EXTERNAL: usize = 1;

/// Whether entry `i` is a generator OOS sends commands to.
pub fn external(i: usize) -> bool {
    i != BUILTIN && i != FREE && i < GENERATORS.len()
}

impl Gen {
    /// Whether the commands of this model name a channel, or the load.
    pub fn has(&self, var: &str) -> bool {
        [self.setup, self.step, self.on, self.off].iter().any(|t| t.contains(var))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_indices_keep_upstreams_presets() {
        let names: Vec<&str> = GENERATORS.iter().map(|g| g.name).collect();
        assert_eq!(names[..6], ["Built-in", "APPL", "BSWV", "SCPI", "Free-running sweep", "CHANnel"]);
        assert_eq!((GENERATORS[BUILTIN].name, GENERATORS[FREE].name), ("Built-in", "Free-running sweep"));
    }
}
