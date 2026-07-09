use crate::api::error::ApiError;

pub(crate) fn naira_to_kobo(amount: i64, field_name: &str) -> Result<i64, ApiError> {
    if amount <= 0 {
        return Err(ApiError::BadRequest(format!(
            "{field_name} must be greater than zero"
        )));
    }

    amount
        .checked_mul(100)
        .ok_or_else(|| ApiError::BadRequest(format!("{field_name} is too large")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_naira_to_kobo() {
        assert_eq!(naira_to_kobo(700, "amount").unwrap(), 70_000);
    }

    #[test]
    fn rejects_non_positive_amount() {
        assert!(matches!(
            naira_to_kobo(0, "amount"),
            Err(ApiError::BadRequest(message)) if message == "amount must be greater than zero"
        ));
    }

    #[test]
    fn rejects_amount_that_overflows_kobo_storage() {
        assert!(matches!(
            naira_to_kobo(i64::MAX, "amount"),
            Err(ApiError::BadRequest(message)) if message == "amount is too large"
        ));
    }
}
