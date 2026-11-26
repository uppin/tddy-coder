use std::path::Path;
use std::sync::Arc;

use tddy_fs::{AsyncFs, Fs, MemFs};

#[tokio::test(flavor = "current_thread")]
async fn async_writes_land_in_the_port_underneath() {
    let mem = Arc::new(MemFs::new());
    let fs = AsyncFs::new(mem.clone());
    fs.write_atomic("/a/b", b"hi".to_vec(), None).await.unwrap();
    assert_eq!(mem.read_to_string(Path::new("/a/b")).unwrap(), "hi");
    assert_eq!(fs.read_to_string("/a/b").await.unwrap(), "hi");
}
