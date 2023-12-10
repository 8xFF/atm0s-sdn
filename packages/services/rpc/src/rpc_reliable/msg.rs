pub const MSG_ACK: u8 = 0;
pub const MSG_DATA: u8 = 1;
pub const RESEND_AFTER_MS: u64 = 200;
pub const RESEND_TIMEOUT_MS: u64 = 3_000;
pub const MAX_PART_LEN: usize = 1100;

/// Converts a tuple of three values into a single 32-bit integer and vice versa.
///
/// # Arguments
///
/// * `msg_id` - The message ID value (u16).
/// * `part_index` - The index of the part (u8).
/// * `part_count_minus_1` - The total count of parts (u8) - 1.
///
/// # Returns
///
/// * `build_stream_id`: A single 32-bit integer representing the combined values of `msg_id`, `part_index`, and `part_count`.
/// * `parse_stream_id`: A tuple containing the extracted values of `msg_id`, `part_index`, and `part_count`.
///
pub fn build_stream_id(msg_id: u16, part_index: u8, part_count_minus_1: u8) -> u32 {
    (msg_id as u32) << 16 | (part_index as u32) << 8 | (part_count_minus_1 as u32)
}

/// Parses a single 32-bit integer into a tuple of three values.
///
/// # Arguments
///
/// * `stream_id` - A single 32-bit integer representing the combined values of `msg_id`, `part_index`, and `part_count_minus_1`.
///
/// # Returns
///
/// A tuple containing the extracted values of `msg_id`, `part_index`, and `part_count_minus_1`.
///
pub fn parse_stream_id(stream_id: u32) -> (u16, u8, u8) {
    ((stream_id >> 16) as u16, (stream_id >> 8) as u8, stream_id as u8)
}

#[cfg(test)]
mod tests {
    use crate::rpc_reliable::msg::{build_stream_id, parse_stream_id};

    #[test]
    fn test_build_stream_id_with_valid_input_values() {
        let msg_id = 0x1234;
        let part_index = 0x04;
        let part_count_minus_1 = 0x09;
        let expected = 0x12340409;

        let result = build_stream_id(msg_id, part_index, part_count_minus_1);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_stream_id_with_valid_stream_id() {
        let stream_id = 0x12340409;
        let expected = (0x1234, 0x04, 0x09);

        let result = parse_stream_id(stream_id);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_build_stream_id_with_maximum_values() {
        let msg_id = std::u16::MAX;
        let part_index = std::u8::MAX;
        let part_count_minus_1 = std::u8::MAX;
        let expected = 0xFFFFFFFF;

        let result = build_stream_id(msg_id, part_index, part_count_minus_1);

        assert_eq!(result, expected);
    }
}
