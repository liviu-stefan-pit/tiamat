use tiamat_contracts::EvidenceClassification;

/// Establish baseline vs post-change classification for a test that may have been failing already.
pub fn classify_baseline(
    baseline_exit: Option<i32>,
    post_exit: i32,
    expected_exit: i32,
) -> EvidenceClassification {
    let post_ok = post_exit == expected_exit;
    match baseline_exit {
        Some(b) if b != expected_exit && !post_ok => EvidenceClassification::BaselineFail,
        Some(b) if b != expected_exit && post_ok => EvidenceClassification::Pass,
        _ if post_ok => EvidenceClassification::Pass,
        _ => EvidenceClassification::Fail,
    }
}

/// Flaky retry default is one; a later pass does not erase the initial failure label.
pub fn classify_flaky_retry(
    first: EvidenceClassification,
    retry_exit: i32,
    expected_exit: i32,
) -> EvidenceClassification {
    let retry_ok = retry_exit == expected_exit;
    match first {
        EvidenceClassification::Fail | EvidenceClassification::FlakyFail if retry_ok => {
            EvidenceClassification::FlakyPass
        }
        EvidenceClassification::Fail | EvidenceClassification::FlakyFail => {
            EvidenceClassification::FlakyFail
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_fail_preserved() {
        assert_eq!(
            classify_baseline(Some(1), 1, 0),
            EvidenceClassification::BaselineFail
        );
        assert_eq!(
            classify_baseline(Some(1), 0, 0),
            EvidenceClassification::Pass
        );
        assert_eq!(classify_baseline(None, 1, 0), EvidenceClassification::Fail);
    }

    #[test]
    fn flaky_pass_keeps_label() {
        assert_eq!(
            classify_flaky_retry(EvidenceClassification::Fail, 0, 0),
            EvidenceClassification::FlakyPass
        );
        assert_eq!(
            classify_flaky_retry(EvidenceClassification::Fail, 1, 0),
            EvidenceClassification::FlakyFail
        );
    }
}
