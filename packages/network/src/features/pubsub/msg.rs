use atm0s_sdn_utils::simple_pub_type;

simple_pub_type!(PubsubChannel, u64);

enum Message {
    Sub(PubsubChannel),
    SubOK(PubsubChannel),
}
