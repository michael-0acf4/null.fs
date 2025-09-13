use crate::netfs::{
    self, NetFs, NetFsPath, any_fs::AnyFs, local_fs::LocalVolume, snapshot::Snapshot,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[test]
fn test_netfs_path() -> eyre::Result<()> {
    let path = NetFsPath::from_to_str("@/a/b/c")?;
    assert_eq!(
        path.components(),
        vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
    );

    assert_eq!(
        NetFsPath::from(&PathBuf::from("a/b/c"))?.to_string(),
        NetFsPath::from_to_str("@/a/b/c")?.to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_snapshot() -> eyre::Result<()> {
    let mut volume = LocalVolume {
        name: "Example".to_owned(),
        root: PathBuf::from("src/tests/test_dir"),
        shares: vec![],
    };
    let local_root = volume.root.clone();
    volume.init().await?;

    let state_file = PathBuf::from("src/tests").join(format!("{}.state.json", volume.name));

    {
        tokio::fs::remove_file(&state_file).await.ok();
        tokio::fs::remove_file(&local_root.join("new_dir/eee.txt"))
            .await
            .ok();
        tokio::fs::remove_file(&local_root.join("new_dir/fff.txt"))
            .await
            .ok();
        tokio::fs::remove_dir(local_root.join("new_dir")).await.ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let snapshot = Snapshot::new(AnyFs::Local { expose: volume });
    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 4);

    {
        tokio::fs::create_dir_all(local_root.join("new_dir")).await?;
        tokio::fs::copy(
            local_root.join("c/d.txt"),
            local_root.join("new_dir/eee.txt"),
        )
        .await?;
        tokio::fs::copy(
            local_root.join("c/d.txt"),
            local_root.join("new_dir/fff.txt"),
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 3);

    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 0);

    {
        tokio::fs::remove_file(&local_root.join("new_dir/fff.txt"))
            .await
            .ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let commands = snapshot.capture(&state_file).await?;
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], netfs::Command::Delete { .. }));
    Ok(())
}
