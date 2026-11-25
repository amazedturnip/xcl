//! Contract tests for `hle/d3d12x/layout.rs` — the PC D3D12 interface table:
//! `find`/`slot` lookups, `chain` version filtering/sorting, and the
//! `parse_guid`/`guid_string` round-trip. No GPU is required.

use xcl::hle::d3d12_layout::IFACES;
use xcl::hle::d3d12x::layout::{chain, find, guid_string, parse_guid, slot};

#[test]
fn chain_sorts_most_derived_first_and_filters_by_digits() {
    let c = chain("ID3D12Device");
    assert!(c.len() >= 10, "expected every ID3D12Device version in the table");

    // every member is exactly the base or the base + all-digit suffix
    for i in &c {
        assert!(
            i.name == "ID3D12Device"
                || (i.name.starts_with("ID3D12Device")
                    && i.name["ID3D12Device".len()..].chars().all(|ch| ch.is_ascii_digit())),
            "unexpected chain member: {}",
            i.name
        );
    }

    // version filter excludes the non-version siblings sharing the base prefix
    assert!(!c.iter().any(|i| i.name == "ID3D12DeviceChild"));
    assert!(!c.iter().any(|i| i.name == "ID3D12DeviceConfiguration"));

    // the base interface itself is present
    assert!(c.iter().any(|i| i.name == "ID3D12Device"));

    // most-derived (highest method count) comes first
    let max = c.iter().map(|i| i.methods.len()).max().unwrap();
    assert_eq!(c[0].methods.len(), max);
    for w in c.windows(2) {
        assert!(w[0].methods.len() >= w[1].methods.len(), "chain not sorted most-derived first: {:?}", c.iter().map(|i| i.name).collect::<Vec<_>>());
    }
    // the head is strictly longer than the tail (base is not the same length as every version)
    assert!(c[0].methods.len() > c[c.len() - 1].methods.len());
}

#[test]
fn chain_of_single_base_has_only_matching_versions() {
    let c = chain("ID3D12CommandQueue");
    assert_eq!(c.len(), 2, "ID3D12CommandQueue + ID3D12CommandQueue1 only");
    assert_eq!(c[0].name, "ID3D12CommandQueue1");
    assert_eq!(c[1].name, "ID3D12CommandQueue");
    assert!(c[0].methods.len() > c[1].methods.len());
}

#[test]
fn find_and_slot_know_interfaces_and_reject_misses() {
    let dev = find("ID3D12Device").expect("ID3D12Device in table");
    assert_eq!(dev.name, "ID3D12Device");
    assert_eq!(slot(dev, "CreateCommandQueue"), Some(8));
    assert_eq!(slot(dev, "CreateCommittedResource"), Some(27));
    assert_eq!(slot(dev, "CreateFence"), Some(36));

    let cl = find("ID3D12GraphicsCommandList").expect("ID3D12GraphicsCommandList in table");
    assert_eq!(slot(cl, "OMSetRenderTargets"), Some(46));

    // method that does not exist on the interface reports None, not a wrong slot
    assert_eq!(slot(dev, "NoSuchMethod"), None);
    assert_eq!(slot(cl, "OMSetRenderTargets2"), None);

    // unknown interface name reports None (find is exact, never a prefix scan)
    assert!(find("ID3D12NotAThing").is_none());
}

#[test]
fn iface_names_and_iids_are_unique() {
    assert!(!IFACES.is_empty(), "empty iface table");
    let mut names: Vec<&str> = IFACES.iter().map(|i| i.name).collect();
    let mut iids: Vec<&str> = IFACES.iter().map(|i| i.iid).collect();
    let name_count = names.len();
    let iid_count = iids.len();
    names.sort_unstable();
    iids.sort_unstable();
    names.dedup();
    iids.dedup();
    assert_eq!(names.len(), name_count, "duplicate interface names: find/slot would be ambiguous");
    assert_eq!(iids.len(), iid_count, "duplicate interface iids: parse/guid_string would be ambiguous");
}

#[test]
fn guid_string_round_trips_every_iface_iid() {
    for i in IFACES {
        let g = parse_guid(i.iid);
        assert_eq!(guid_string(&g), i.iid, "round-trip failed for {}", i.name);
    }
}

#[test]
fn parse_guid_canonical_iid_matches_hex_string() {
    let g = parse_guid("189819f1-1db6-4b57-be54-1821339b85f7");
    assert_eq!(g.data1, 0x189819f1);
    assert_eq!(g.data2, 0x1db6);
    assert_eq!(g.data3, 0x4b57);
    assert_eq!(g.data4, [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7]);

    // the ID3D12Device iid parses to the same fields (the round-trip test independently
    // checks guid_string(parse_guid(iid)) for every IFACE entry, including this one)
    let dev = find("ID3D12Device").unwrap();
    let d = parse_guid(dev.iid);
    assert_eq!(d.data1, g.data1);
    assert_eq!(d.data2, g.data2);
    assert_eq!(d.data3, g.data3);
    assert_eq!(d.data4, g.data4);
}

#[test]
fn parse_guid_garbage_falls_back_to_zero_and_never_panics() {
    // empty string
    assert_eq!(parse_guid("").data1, 0);
    assert_eq!(parse_guid("").data4, [0, 0, 0, 0, 0, 0, 0, 0]);

    // no hex digits at all
    assert_eq!(parse_guid("zzz zzz").data1, 0);
    assert_eq!(parse_guid("zzz zzz").data4, [0, 0, 0, 0, 0, 0, 0, 0]);

    // non-hex characters are dropped, valid hex nibbles survive
    // ("not a guid" has hex digits 'a' and 'd' -> hex "ad" = 0xad).
    // NOTE: this pins the current windows_sys GUID::from_u128 bit-mapping as a
    // characterization — the contract under test is that parse_guid never panics
    // and falls back sanely; if windows-rs changes from_u128 semantics this may
    // need updating, not layout.rs.
    assert_eq!(parse_guid("not a guid").data4, [0, 0, 0, 0, 0, 0, 0, 0xad]);

    // too many hex digits to fit a GUID -> falls back to zero
    let g = parse_guid(&"1".repeat(40));
    assert_eq!(g.data1, 0);
    assert_eq!(g.data4, [0, 0, 0, 0, 0, 0, 0, 0]);

    // every IFACE entry parses without panicking (round-trip is checked elsewhere)
    for i in IFACES {
        let _ = parse_guid(i.iid);
    }
}
