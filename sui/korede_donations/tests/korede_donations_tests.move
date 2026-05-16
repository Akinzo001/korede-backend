#[test_only]
module korede_donations::korede_donations_tests;

use korede_donations::korede_donations;

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
