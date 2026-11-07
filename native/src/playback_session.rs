use serde::Deserialize;

/// The loaded playback document is authoritative, independently of API cookies.
#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub signed_in: Option<bool>,
    pub auth_user: Option<u8>,
}

pub fn allowed(expected_account: bool, identity: Identity) -> bool {
    !expected_account || identity.signed_in == Some(true)
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
}
