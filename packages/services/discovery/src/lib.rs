mod behavior;
mod closest_list;
mod connection_group;
mod handler;
pub(crate) mod kbucket;
mod logic;
mod msg;

#[cfg(test)]
mod tests {
    #[async_std::test]
    async fn bootstrap() {

    }
}
