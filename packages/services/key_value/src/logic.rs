/// This storage is used to store key-value pairs in a database.
/// Logic: external runtime call actions or tick then pull actions from the queue and execute them.
/// This storage implement bellow types:
///     - Simple Key Value with version
///     - Multi Key Value with version
///
/// Two types of data
///     - Local Data ( this is used to store data in the current runtime )
///     - Remote Data ( this is used to store data from other nodes )
///
/// Two types of output actions
///     - SimpleKeyValue(action)
///     - MultiKeyValue(action)
///
/// Action and Ack: Each action has uuid and when action is executed, it will create an ack with
/// the same uuid and response to source of action. If ack is not received in a period of time, the action will be executed again.
pub mod key_value;
