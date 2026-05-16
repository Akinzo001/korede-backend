module korede_donations::korede_donations;

use std::vector;
use sui::clock::{Self, Clock};
use sui::event;
use sui::object::{Self, ID, UID};
use sui::transfer;
use sui::tx_context::{Self, TxContext};

const E_INVALID_AMOUNT: u64 = 1;
const E_EMPTY_CASE_ID: u64 = 2;
const E_EMPTY_HOSPITAL_ID: u64 = 3;
const E_EMPTY_PAYMENT_REFERENCE: u64 = 4;

public struct DonationRecord has key, store {
    id: UID,
    donor: address,
    authority: address,
    case_id: vector<u8>,
    hospital_id: vector<u8>,
    amount_kobo: u64,
    payment_reference: vector<u8>,
    recorded_at_ms: u64,
}

public struct DonationRecorded has copy, drop {
    donor: address,
    authority: address,
    case_id: vector<u8>,
    hospital_id: vector<u8>,
    amount_kobo: u64,
    payment_reference: vector<u8>,
    recorded_at_ms: u64,
    record_object_id: ID,
}

public entry fun record_donation(
    donor: address,
    case_id: vector<u8>,
    hospital_id: vector<u8>,
    amount_kobo: u64,
    payment_reference: vector<u8>,
    clock: &Clock,
    ctx: &mut TxContext,
) {
    validate_inputs(&case_id, &hospital_id, amount_kobo, &payment_reference);

    let authority = tx_context::sender(ctx);
    let recorded_at_ms = clock::timestamp_ms(clock);
    let record = DonationRecord {
        id: object::new(ctx),
        donor,
        authority,
        case_id,
        hospital_id,
        amount_kobo,
        payment_reference,
        recorded_at_ms,
    };
    let record_object_id = object::id(&record);

    event::emit(DonationRecorded {
        donor,
        authority,
        case_id: record.case_id,
        hospital_id: record.hospital_id,
        amount_kobo,
        payment_reference: record.payment_reference,
        recorded_at_ms,
        record_object_id,
    });

    transfer::transfer(record, authority);
}

fun validate_inputs(
    case_id: &vector<u8>,
    hospital_id: &vector<u8>,
    amount_kobo: u64,
    payment_reference: &vector<u8>,
) {
    assert!(amount_kobo > 0, E_INVALID_AMOUNT);
    assert!(!vector::is_empty(case_id), E_EMPTY_CASE_ID);
    assert!(!vector::is_empty(hospital_id), E_EMPTY_HOSPITAL_ID);
    assert!(
        !vector::is_empty(payment_reference),
        E_EMPTY_PAYMENT_REFERENCE,
    );
}

#[test_only]
public fun assert_valid_inputs_for_tests(
    case_id: vector<u8>,
    hospital_id: vector<u8>,
    amount_kobo: u64,
    payment_reference: vector<u8>,
) {
    validate_inputs(&case_id, &hospital_id, amount_kobo, &payment_reference);
}
