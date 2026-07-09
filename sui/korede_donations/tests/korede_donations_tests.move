#[test_only]
module korede_donations::korede_donations_tests;

use korede_donations::korede_donations;

#[test]
fun amount_is_split_into_naira_and_kobo() {
    let (amount, kobo) = korede_donations::split_amount_for_tests(70_000);
    assert!(amount == 700, 0);
    assert!(kobo == 0, 1);

    let (amount_with_fraction, fractional_kobo) =
        korede_donations::split_amount_for_tests(70_050);
    assert!(amount_with_fraction == 700, 2);
    assert!(fractional_kobo == 50, 3);
}

#[test]
fun valid_inputs_are_accepted() {
    korede_donations::assert_valid_inputs_for_tests(
        b"case-id-hash",
        b"hospital-id-hash",
        10_000,
        b"payment-reference-hash",
    );
}

#[test, expected_failure(abort_code = 1)]
fun zero_amount_is_rejected() {
    korede_donations::assert_valid_inputs_for_tests(
        b"case-id-hash",
        b"hospital-id-hash",
        0,
        b"payment-reference-hash",
    );
}

#[test, expected_failure(abort_code = 2)]
fun empty_case_id_is_rejected() {
    korede_donations::assert_valid_inputs_for_tests(
        b"",
        b"hospital-id-hash",
        10_000,
        b"payment-reference-hash",
    );
}

#[test, expected_failure(abort_code = 3)]
fun empty_hospital_id_is_rejected() {
    korede_donations::assert_valid_inputs_for_tests(
        b"case-id-hash",
        b"",
        10_000,
        b"payment-reference-hash",
    );
}

#[test, expected_failure(abort_code = 4)]
fun empty_payment_reference_is_rejected() {
    korede_donations::assert_valid_inputs_for_tests(
        b"case-id-hash",
        b"hospital-id-hash",
        10_000,
        b"",
    );
}
