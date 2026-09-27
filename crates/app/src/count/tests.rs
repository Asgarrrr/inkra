use super::thousands;

#[test]
fn a_count_groups_its_digits_by_three() {
    let cases = [
        (0, "0"),
        (999, "999"),
        (1_000, "1,000"),
        (12_340, "12,340"),
        (50_000, "50,000"),
        (1_234_567, "1,234,567"),
    ];
    for (n, text) in cases {
        assert_eq!(thousands(n), text, "{n}");
    }
}
