use crate::netfs::{self, NetFs, local_fs::LocalVolume, snapshot::Snapshot};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[tokio::test]
async fn test_snapshot() -> eyre::Result<()> {
    let mut volume = LocalVolume {
        name: "Example".to_owned(),
        root: PathBuf::from("src/tests/test_dir"),
        shares: vec![],
    };
    volume.init().await?;
    let volume = Arc::new(volume);

    let state_file = PathBuf::from("src/tests").join(format!("{}.state.json", volume.name));

    {
        tokio::fs::remove_file(&state_file).await.ok();
        tokio::fs::remove_file(&volume.clone().root.join("new_dir/eee.txt"))
            .await
            .ok();
        tokio::fs::remove_file(&volume.clone().root.join("new_dir/fff.txt"))
            .await
            .ok();
        tokio::fs::remove_dir(volume.clone().root.join("new_dir"))
            .await
            .ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let snapshot = Snapshot::new(volume.clone());
    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 4);

    {
        tokio::fs::create_dir_all(volume.clone().root.join("new_dir")).await?;
        tokio::fs::copy(
            volume.clone().root.join("c/d.txt"),
            volume.clone().root.join("new_dir/eee.txt"),
        )
        .await?;
        tokio::fs::copy(
            volume.clone().root.join("c/d.txt"),
            volume.clone().root.join("new_dir/fff.txt"),
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 3);

    let commands = snapshot.clone().capture(&state_file).await?;
    assert_eq!(commands.len(), 0);

    {
        tokio::fs::remove_file(&volume.clone().root.join("new_dir/fff.txt"))
            .await
            .ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let commands = snapshot.capture(&state_file).await?;
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], netfs::Command::Delete { .. }));
    Ok(())
}
