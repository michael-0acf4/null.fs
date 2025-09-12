use crate::netfs::{self, NetFs, NetFsPath, local_fs::LocalVolume, snapshot::Snapshot};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[test]
fn test_netfs_path() -> eyre::Result<()> {
    let win_path = r"D:\a\b\c";
    let path = NetFsPath::from_to_str(win_path)?;
    let win_path_normalized_reconstructed = path.to_host_path();

    assert_eq!("/@win-d/a/b/c", path.to_string());
    assert_eq!(
        "D:/a/b/c",
        win_path_normalized_reconstructed.display().to_string()
    );
    assert_eq!(
        "/@win-d/a/b/c",
        NetFsPath::from(&win_path_normalized_reconstructed)?.to_string()
    );

    let rel = NetFsPath::from_to_str("src/tests/test_dir")?;
    assert_eq!("/@win-d/a/b/c", rel.to_string());

    Ok(())
}

#[tokio::test]
async fn test_snapshot() -> eyre::Result<()> {
    let mut volume = LocalVolume {
        name: "Example".to_owned(),
        root: NetFsPath::from_to_str("src/tests/test_dir")?,
        shares: vec![],
    };
    volume.init().await?;
    let volume = Arc::new(volume);

    let state_file = PathBuf::from("src/tests").join(format!("{}.state.json", volume.name));

    {
        tokio::fs::remove_file(&state_file).await.ok();
        tokio::fs::remove_file(&volume.clone().root.join("new_dir/eee.txt")?)
            .await
            .ok();
        tokio::fs::remove_file(&volume.clone().root.join("new_dir/fff.txt")?)
            .await
            .ok();
        tokio::fs::remove_dir(volume.clone().root.join("new_dir")?)
            .await
            .ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let snapshot = Snapshot::new(volume.clone());
    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 4);

    {
        tokio::fs::create_dir_all(volume.clone().root.join("new_dir")?).await?;
        tokio::fs::copy(
            volume.clone().root.join("c/d.txt")?,
            volume.clone().root.join("new_dir/eee.txt")?,
        )
        .await?;
        tokio::fs::copy(
            volume.clone().root.join("c/d.txt")?,
            volume.clone().root.join("new_dir/fff.txt")?,
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 3);

    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 0);

    {
        tokio::fs::remove_file(&volume.clone().root.join("new_dir/fff.txt")?)
            .await
            .ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let commands = snapshot.capture(&state_file).await?;
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], netfs::Command::Delete { .. }));
    Ok(())
}
