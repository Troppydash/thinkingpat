use std::fs;

#[macro_export]
macro_rules! versions {
    ($name:ident, $($version:ident),*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $name {
            $($version,)*
        }

        impl ToString for $name {
            fn to_string(&self) -> String {
                match *self {
                    $(
                        $name::$version => format!("{}/{}", stringify!($name), stringify!($version)),

                    )*
                }
            }
        }
    };
}
pub trait Versioned<V: ToString> {
    fn build(version: &V, path: &str) -> Self;
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(version: &V, path: &str, bytes: &[u8]) -> Self;
}

fn get_path<V: ToString, T: Versioned<V>>(version: &V, path: &str) -> String {
    format!("{}/{}", path, version.to_string())
}

pub fn try_load<V: ToString, T: Versioned<V>>(version: &V, path: &str) -> T {
    let direct_path = get_path::<V, T>(&version, path);
    if let Ok(bytes) = fs::read(&direct_path) {
        return T::from_bytes(version, path, &bytes);
    }

    let t = T::build(&version, &path);
    fs::write(&direct_path, t.to_bytes()).unwrap();
    t
}

pub fn load<V: ToString, T: Versioned<V>>(version: &V, path: &str) -> T {
    T::build(&version, &path)
}

#[allow(dead_code)]
#[cfg(test)]
mod test {
    use super::*;

    struct Test {}

    versions!(TestVersion, V1, V2);

    impl Versioned<TestVersion> for Test {
        fn to_bytes(&self) -> Vec<u8> {
            todo!()
        }

        fn from_bytes(_version: &TestVersion, path: &str, _bytes: &[u8]) -> Self {
            todo!()
        }

        fn build(version: &TestVersion, path: &str) -> Self {
            if *version == TestVersion::V2 {
                try_load(&TestVersion::V1, path)
            } else {
                todo!()
            }
        }
    }
}
