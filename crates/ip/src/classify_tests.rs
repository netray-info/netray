#[cfg(test)]
mod tests {
    //! C2: one network-type classifier and its priority order.

    use crate::backend::{NetworkFlags, classify_network_type};

    fn flags(f: impl FnOnce(&mut NetworkFlags)) -> NetworkFlags {
        let mut n = NetworkFlags::default();
        f(&mut n);
        n
    }

    #[test]
    fn classify_network_type_priority_table() {
        let cases: Vec<(&str, NetworkFlags, &str)> = vec![
            ("none", NetworkFlags::default(), "residential"),
            (
                "internal+c2",
                flags(|f| {
                    f.is_internal = true;
                    f.is_c2 = true;
                }),
                "internal",
            ),
            (
                "c2+bot",
                flags(|f| {
                    f.is_c2 = true;
                    f.is_bot = true;
                }),
                "c2",
            ),
            (
                "bot+cloud",
                flags(|f| {
                    f.is_bot = true;
                    f.is_cloud = true;
                }),
                "bot",
            ),
            (
                "cloud+vpn",
                flags(|f| {
                    f.is_cloud = true;
                    f.is_vpn = true;
                }),
                "cloud",
            ),
            (
                "vpn+tor",
                flags(|f| {
                    f.is_vpn = true;
                    f.is_tor = true;
                }),
                "vpn",
            ),
            (
                "tor+spamhaus",
                flags(|f| {
                    f.is_tor = true;
                    f.is_spamhaus = true;
                }),
                "tor",
            ),
            (
                "spamhaus+datacenter",
                flags(|f| {
                    f.is_spamhaus = true;
                    f.is_datacenter = true;
                }),
                "spamhaus",
            ),
            ("datacenter", flags(|f| f.is_datacenter = true), "datacenter"),
        ];
        for (name, f, want) in cases {
            assert_eq!(classify_network_type(f), want, "case {name}");
        }
    }
}
