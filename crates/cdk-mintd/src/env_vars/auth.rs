//! Auth env

use std::env;

use crate::config::Auth;

pub const ENV_AUTH_OPENID_DISCOVERY: &str = "CDK_MINTD_AUTH_OPENID_DISCOVERY";
pub const ENV_AUTH_OPENID_CLIENT_ID: &str = "CDK_MINTD_AUTH_OPENID_CLIENT_ID";
pub const ENV_AUTH_MINT_MAX_BAT: &str = "CDK_MINTD_AUTH_MINT_MAX_BAT";
pub const ENV_AUTH_ENABLED_MINT: &str = "CDK_MINTD_AUTH_ENABLED_MINT";
pub const ENV_AUTH_ENABLED_MELT: &str = "CDK_MINTD_AUTH_ENABLED_MELT";
pub const ENV_AUTH_ENABLED_SWAP: &str = "CDK_MINTD_AUTH_ENABLED_SWAP";
pub const ENV_AUTH_ENABLED_GET_MINT_QUOTE: &str = "CDK_MINTD_AUTH_ENABLED_GET_MINT_QUOTE";
pub const ENV_AUTH_ENABLED_CHECK_MINT_QUOTE: &str = "CDK_MINTD_AUTH_ENABLED_CHECK_MINT_QUOTE";
pub const ENV_AUTH_ENABLED_GET_MELT_QUOTE: &str = "CDK_MINTD_AUTH_ENABLED_GET_MELT_QUOTE";
pub const ENV_AUTH_ENABLED_CHECK_MELT_QUOTE: &str = "CDK_MINTD_AUTH_ENABLED_CHECK_MELT_QUOTE";
pub const ENV_AUTH_ENABLED_RESTORE: &str = "CDK_MINTD_AUTH_ENABLED_RESTORE";
pub const ENV_AUTH_ENABLED_CHECK_PROOF_STATE: &str = "CDK_MINTD_AUTH_ENABLED_CHECK_PROOF_STATE";
pub const ENV_AUTH_MINT_AUTH_TYPE: &str = "CDK_MINTD_AUTH_MINT_AUTH_TYPE";
pub const ENV_AUTH_MELT_AUTH_TYPE: &str = "CDK_MINTD_AUTH_MELT_AUTH_TYPE";
pub const ENV_AUTH_SWAP_AUTH_TYPE: &str = "CDK_MINTD_AUTH_SWAP_AUTH_TYPE";
pub const ENV_AUTH_GET_MINT_QUOTE_AUTH_TYPE: &str = "CDK_MINTD_AUTH_GET_MINT_QUOTE_AUTH_TYPE";
pub const ENV_AUTH_CHECK_MINT_QUOTE_AUTH_TYPE: &str = "CDK_MINTD_AUTH_CHECK_MINT_QUOTE_AUTH_TYPE";
pub const ENV_AUTH_GET_MELT_QUOTE_AUTH_TYPE: &str = "CDK_MINTD_AUTH_GET_MELT_QUOTE_AUTH_TYPE";
pub const ENV_AUTH_CHECK_MELT_QUOTE_AUTH_TYPE: &str = "CDK_MINTD_AUTH_CHECK_MELT_QUOTE_AUTH_TYPE";
pub const ENV_AUTH_RESTORE_AUTH_TYPE: &str = "CDK_MINTD_AUTH_RESTORE_AUTH_TYPE";
pub const ENV_AUTH_CHECK_PROOF_STATE_AUTH_TYPE: &str = "CDK_MINTD_AUTH_CHECK_PROOF_STATE_AUTH_TYPE";

impl Auth {
    pub fn from_env(mut self) -> Self {
        if let Ok(discovery) = env::var(ENV_AUTH_OPENID_DISCOVERY) {
            self.openid_discovery = discovery;
        }

        if let Ok(client_id) = env::var(ENV_AUTH_OPENID_CLIENT_ID) {
            self.openid_client_id = client_id;
        }

        if let Ok(max_bat_str) = env::var(ENV_AUTH_MINT_MAX_BAT) {
            if let Ok(max_bat) = max_bat_str.parse() {
                self.mint_max_bat = max_bat;
            }
        }

        if let Ok(enabled_mint_str) = env::var(ENV_AUTH_ENABLED_MINT) {
            if let Ok(enabled) = enabled_mint_str.parse() {
                self.enabled_mint = enabled;
            }
        }

        if let Ok(enabled_melt_str) = env::var(ENV_AUTH_ENABLED_MELT) {
            if let Ok(enabled) = enabled_melt_str.parse() {
                self.enabled_melt = enabled;
            }
        }

        if let Ok(enabled_swap_str) = env::var(ENV_AUTH_ENABLED_SWAP) {
            if let Ok(enabled) = enabled_swap_str.parse() {
                self.enabled_swap = enabled;
            }
        }

        if let Ok(enabled_get_mint_str) = env::var(ENV_AUTH_ENABLED_GET_MINT_QUOTE) {
            if let Ok(enabled) = enabled_get_mint_str.parse() {
                self.enabled_get_mint_quote = enabled;
            }
        }

        if let Ok(enabled_check_mint_str) = env::var(ENV_AUTH_ENABLED_CHECK_MINT_QUOTE) {
            if let Ok(enabled) = enabled_check_mint_str.parse() {
                self.enabled_check_mint_quote = enabled;
            }
        }

        if let Ok(enabled_get_melt_str) = env::var(ENV_AUTH_ENABLED_GET_MELT_QUOTE) {
            if let Ok(enabled) = enabled_get_melt_str.parse() {
                self.enabled_get_melt_quote = enabled;
            }
        }

        if let Ok(enabled_check_melt_str) = env::var(ENV_AUTH_ENABLED_CHECK_MELT_QUOTE) {
            if let Ok(enabled) = enabled_check_melt_str.parse() {
                self.enabled_check_melt_quote = enabled;
            }
        }

        if let Ok(enabled_restore_str) = env::var(ENV_AUTH_ENABLED_RESTORE) {
            if let Ok(enabled) = enabled_restore_str.parse() {
                self.enabled_restore = enabled;
            }
        }

        if let Ok(enabled_check_proof_str) = env::var(ENV_AUTH_ENABLED_CHECK_PROOF_STATE) {
            if let Ok(enabled) = enabled_check_proof_str.parse() {
                self.enabled_check_proof_state = enabled;
            }
        }

        if let Ok(mint_auth_type_str) = env::var(ENV_AUTH_MINT_AUTH_TYPE) {
            if let Ok(auth_type) = mint_auth_type_str.parse() {
                self.mint_auth_type = auth_type;
            }
        }

        if let Ok(melt_auth_type_str) = env::var(ENV_AUTH_MELT_AUTH_TYPE) {
            if let Ok(auth_type) = melt_auth_type_str.parse() {
                self.melt_auth_type = auth_type;
            }
        }

        if let Ok(swap_auth_type_str) = env::var(ENV_AUTH_SWAP_AUTH_TYPE) {
            if let Ok(auth_type) = swap_auth_type_str.parse() {
                self.swap_auth_type = auth_type;
            }
        }

        if let Ok(get_mint_quote_auth_type_str) = env::var(ENV_AUTH_GET_MINT_QUOTE_AUTH_TYPE) {
            if let Ok(auth_type) = get_mint_quote_auth_type_str.parse() {
                self.get_mint_quote_auth_type = auth_type;
            }
        }

        if let Ok(check_mint_quote_auth_type_str) = env::var(ENV_AUTH_CHECK_MINT_QUOTE_AUTH_TYPE) {
            if let Ok(auth_type) = check_mint_quote_auth_type_str.parse() {
                self.check_mint_quote_auth_type = auth_type;
            }
        }

        if let Ok(get_melt_quote_auth_type_str) = env::var(ENV_AUTH_GET_MELT_QUOTE_AUTH_TYPE) {
            if let Ok(auth_type) = get_melt_quote_auth_type_str.parse() {
                self.get_melt_quote_auth_type = auth_type;
            }
        }

        if let Ok(check_melt_quote_auth_type_str) = env::var(ENV_AUTH_CHECK_MELT_QUOTE_AUTH_TYPE) {
            if let Ok(auth_type) = check_melt_quote_auth_type_str.parse() {
                self.check_melt_quote_auth_type = auth_type;
            }
        }

        if let Ok(restore_auth_type_str) = env::var(ENV_AUTH_RESTORE_AUTH_TYPE) {
            if let Ok(auth_type) = restore_auth_type_str.parse() {
                self.restore_auth_type = auth_type;
            }
        }

        if let Ok(check_proof_state_auth_type_str) = env::var(ENV_AUTH_CHECK_PROOF_STATE_AUTH_TYPE)
        {
            if let Ok(auth_type) = check_proof_state_auth_type_str.parse() {
                self.check_proof_state_auth_type = auth_type;
            }
        }

        self
    }
}
