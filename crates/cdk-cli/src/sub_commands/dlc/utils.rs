use dlc_messages::oracle_msgs::OracleAnnouncement;
use lightning::util::ser::Readable;
use nostr_sdk::base64;
use std::io::Cursor;

fn decode_bytes(str: &str) -> Result<Vec<u8>, base64::DecodeError> {
    // match FromHex::from_hex(str) {
    //     Ok(bytes) => Ok(bytes),
    //     Err(_) => Ok(base64::decode(str)?),
    // }
    base64::decode(str)
}

/// Parses a string into an oracle announcement.
pub fn oracle_announcement_from_str(str: &str) -> OracleAnnouncement {
    let bytes = decode_bytes(str).expect("Could not decode oracle announcement string");
    let mut cursor = Cursor::new(bytes);

    OracleAnnouncement::read(&mut cursor).expect("Could not parse oracle announcement")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use cdk::secp256k1::schnorr::Signature;
    use dlc_messages::oracle_msgs::EventDescriptor;

    const ANNOUNCEMENT: &str = "ypyyyX6pdZUM+OovHftxK9StImd8F7nxmr/eTeyR/5koOVVe/EaNw1MAeJm8LKDV1w74Fr+UJ+83bVP3ynNmjwKbtJr9eP5ie2Exmeod7kw4uNsuXcw6tqJF1FXH3fTF/dgiOwAByEOAEd95715DKrSLVdN/7cGtOlSRTQ0/LsW/p3BiVOdlpccA/dgGDAACBDEyMzQENDU2NwR0ZXN0";

    #[test]
    fn test_decode_oracle_announcement() {
        let announcement = oracle_announcement_from_str(ANNOUNCEMENT);

        assert_eq!(
            announcement.announcement_signature,
            Signature::from_str(&String::from("ca9cb2c97ea975950cf8ea2f1dfb712bd4ad22677c17b9f19abfde4dec91ff992839555efc468dc353007899bc2ca0d5d70ef816bf9427ef376d53f7ca73668f")).unwrap()
        );

        let descriptor = announcement.oracle_event.event_descriptor;

        match descriptor {
            EventDescriptor::EnumEvent(e) => {
                assert_eq!(e.outcomes.len(), 2);
            }
            EventDescriptor::DigitDecompositionEvent(e) => unreachable!(),
        }
    }
}
