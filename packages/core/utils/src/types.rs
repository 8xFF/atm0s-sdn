#[macro_export]
macro_rules! simple_pub_type {
    ($name:ident, $inner:ty) => {
        #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        pub struct $name(pub $inner);

        impl std::ops::Deref for $name {
            type Target = $inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                $name(value)
            }
        }

        impl Into<$inner> for $name {
            fn into(self) -> $inner {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

#[macro_export]
macro_rules! simple_type {
    ($name:ident, $inner:ty) => {
        #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        struct $name(pub $inner);

        impl std::ops::Deref for $name {
            type Target = $inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                $name(value)
            }
        }

        impl Into<$inner> for $name {
            fn into(self) -> $inner {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "$name({})", self.0)
            }
        }
    };
}
