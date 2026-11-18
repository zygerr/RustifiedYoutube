use serde::Deserialize;

/// The loaded playback document is authoritative, independently of API cookies.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub signed_in: Option<bool>,
    pub auth_user: Option<u8>,
}

pub fn allowed(expected_account: bool, identity: Identity) -> bool {
    !expected_account || identity.signed_in == Some(true)
}

/// Merge an observation from the same document without turning unavailable
/// configuration into a logout. Callers reset identity when documents change.
pub fn observed(previous: Identity, next: Identity) -> Identity {
    match next.signed_in {
        None => previous,
        Some(false) => Identity {
            signed_in: Some(false),
            auth_user: None,
        },
        Some(true) => Identity {
            signed_in: Some(true),
            auth_user: next.auth_user.or_else(|| {
                (previous.signed_in == Some(true))
                    .then_some(previous.auth_user)
                    .flatten()
            }),
        },
    }
}

/// Cookie callbacks are asynchronous: a completed capture must still belong
/// to the document and account that requested it before it can update storage.
pub fn capture_is_current(
    captured_document: Option<&str>,
    current_document: Option<&str>,
    captured_identity: Identity,
    current_identity: Identity,
) -> bool {
    matches!((captured_document, current_document), (Some(captured), Some(current)) if captured == current)
        && captured_identity == current_identity
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_accounts_wait_for_the_actual_playback_login() {
        for signed_in in [None, Some(false)] {
            assert!(!allowed(
                true,
                Identity {
                    signed_in,
                    auth_user: None
                }
            ));
        }
        assert!(allowed(
            true,
            Identity {
                signed_in: Some(true),
                auth_user: Some(1)
            }
        ));
    }
    #[test]
    fn explicit_guest_playback_is_allowed() {
        for signed_in in [None, Some(false), Some(true)] {
            assert!(allowed(
                false,
                Identity {
                    signed_in,
                    auth_user: None
                }
            ));
        }
    }

    #[test]
    fn temporary_unknown_observation_keeps_authenticated_playback_allowed() {
        let connected = Identity {
            signed_in: Some(true),
            auth_user: Some(2),
        };
        let interrupted = observed(connected, Identity::default());
        assert!(interrupted == connected);
        assert!(allowed(true, interrupted));
        assert!(observed(interrupted, connected) == connected);

        // A new document starts with no previous confirmation.
        let initial = observed(Identity::default(), Identity::default());
        assert!(initial == Identity::default());
        assert!(!allowed(true, initial));
    }

    #[test]
    fn account_selector_survives_only_the_same_confirmed_login() {
        let connected = Identity {
            signed_in: Some(true),
            auth_user: Some(2),
        };
        let missing_selector = Identity {
            signed_in: Some(true),
            auth_user: None,
        };
        assert!(observed(connected, missing_selector) == connected);

        let switched = Identity {
            signed_in: Some(true),
            auth_user: Some(1),
        };
        assert!(observed(connected, switched) == switched);

        let signed_out = observed(
            connected,
            Identity {
                signed_in: Some(false),
                auth_user: Some(2),
            },
        );
        assert_eq!(signed_out.signed_in, Some(false));
        assert_eq!(signed_out.auth_user, None);
        assert!(!allowed(true, signed_out));
        assert_eq!(observed(signed_out, missing_selector).auth_user, None);
        assert_eq!(
            observed(Identity::default(), missing_selector).auth_user,
            None
        );
    }

    #[test]
    fn cookie_capture_cannot_cross_a_document_or_account_change() {
        let connected = Identity {
            signed_in: Some(true),
            auth_user: Some(2),
        };
        assert!(capture_is_current(
            Some("document-a"),
            Some("document-a"),
            connected,
            connected
        ));
        assert!(capture_is_current(
            Some("document-a"),
            Some("document-a"),
            Identity::default(),
            Identity::default()
        ));
        for documents in [
            (Some("document-a"), Some("document-b")),
            (Some("document-a"), None),
            (None, Some("document-a")),
            (None, None),
        ] {
            assert!(!capture_is_current(
                documents.0,
                documents.1,
                connected,
                connected
            ));
        }
        for next in [
            Identity::default(),
            Identity {
                signed_in: Some(true),
                auth_user: Some(1),
            },
            Identity {
                signed_in: Some(false),
                auth_user: None,
            },
        ] {
            assert!(!capture_is_current(
                Some("document-a"),
                Some("document-a"),
                connected,
                next
            ));
        }
        assert!(!capture_is_current(
            Some("document-a"),
            Some("document-a"),
            Identity::default(),
            connected
        ));
    }
}
