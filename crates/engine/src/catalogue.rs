//! The catalogue (spec §5): every mode the engine can name or decode, from what its detectors and decoders actually
//! do. Each entry's labels are the claim labels that mode produces, so an interface can count what it has named.
//! When a detector or decoder gains a mode, it gets an entry here; the test below checks the sync bank's protocols
//! are all listed.

use api::{BandSpan, CatalogueEntry, Domain, Reach};

const fn entry(mode: &'static str, domain: Domain, reach: Reach, blurb: &'static str, labels: &'static [&'static str]) -> CatalogueEntry {
    CatalogueEntry { mode, domain, reach, blurb, labels }
}

/// The catalogue, grouped by domain.
pub fn entries() -> Vec<CatalogueEntry> {
    use Domain::*;
    use Reach::*;
    vec![
        entry(
            "P25 Phase 1",
            Trunking,
            Decodes,
            "APCO-25 4FSK at 4800 Bd: NAC, and from a control channel the site, system and grants",
            &["P25 Phase 1"],
        ),
        entry("P25 Phase 2", Trunking, Names, "APCO-25 TDMA at 6000 Bd", &["P25 Phase 2"]),
        entry(
            "DMR",
            Trunking,
            Decodes,
            "ETSI DMR tier II and Capacity Plus: colour code, CSBKs, voice calls with talkgroup and radio",
            &["DMR"],
        ),
        entry("NXDN", Trunking, Decodes, "NXDN48 and NXDN96: RAN and LICH", &["NXDN"]),
        entry("ProVoice", Trunking, Names, "EDACS ProVoice at 9600 bit/s", &["ProVoice"]),
        entry("X2-TDMA", Trunking, Names, "Motorola X2-TDMA", &["X2-TDMA"]),
        entry("dPMR", LandMobile, Names, "ETSI dPMR 4FSK at 2400 Bd", &["dPMR"]),
        entry("System Fusion", LandMobile, Names, "Yaesu System Fusion (C4FM)", &["YSF"]),
        entry("Analog FM", LandMobile, Shape, "Narrowband FM voice, with its CTCSS tone or DCS code", &["Analog FM"]),
        entry("Air band voice", Aviation, Shape, "AM voice on the 25 kHz channels", &["Air band voice", "AM"]),
        entry("ACARS", Aviation, Decodes, "Aircraft datalink (MSK on AM): registration, flight and message", &["ACARS"]),
        entry("POCSAG", Pagers, Decodes, "Pages at 512, 1200 and 2400 bit/s: address, function and text", &["POCSAG"]),
        entry("FLEX", Pagers, Decodes, "FLEX at 1600 and 3200 baud, 2 or 4 levels: capcode and message, simulcast included", &["FLEX"]),
        entry("FM broadcast", Broadcast, Shape, "Wideband FM stations, stereo pilot and RDS", &["FM broadcast"]),
        entry("GSM", Cellular, Decodes, "GSM base stations and their BSIC", &["GSM"]),
        entry("Wi-SUN", Other, Names, "802.15.4g SUN FSK smart-meter mesh", &["802.15.4g"]),
        entry("LoRa", Other, Shape, "Chirp spread spectrum: bandwidth and spreading factor", &["LoRa"]),
    ]
}

/// The band plan's allocations, for the band strip.
pub fn band_plan() -> Vec<BandSpan> {
    detect::bandplan::BANDS.iter().map(|b| BandSpan { lo_hz: b.lo_hz, hi_hz: b.hi_hz, name: b.name, service: b.service.name() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_protocol_the_sync_bank_names_is_in_the_catalogue() {
        let cat = entries();
        for s in detect::sync::table() {
            let label = detect::sync::label(s.protocol, s.detail);
            assert!(cat.iter().any(|e| e.matches(&label)), "{label} is not in the catalogue");
        }
    }
}
