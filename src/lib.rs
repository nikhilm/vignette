#[cfg(target_os = "linux")]
mod lib_linux;
#[cfg(target_os = "linux")]
use lib_linux::*;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
