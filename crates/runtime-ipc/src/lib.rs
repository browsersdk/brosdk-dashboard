use std::{io, time::Duration};

use domain::HostWireMessage;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("IPC I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("IPC frame is too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("IPC message is invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("timed out connecting to runtime host at {0}")]
    ConnectTimeout(String),
}

pub async fn write_message<W>(writer: &mut W, message: &HostWireMessage) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(message)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge(payload.len()));
    }
    writer.write_u32(payload.len() as u32).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_message<R>(reader: &mut R) -> Result<Option<HostWireMessage>, IpcError>
where
    R: AsyncRead + Unpin,
{
    let size = match reader.read_u32().await {
        Ok(size) => size as usize,
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if size > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge(size));
    }
    let mut payload = vec![0_u8; size];
    reader.read_exact(&mut payload).await?;
    Ok(Some(serde_json::from_slice(&payload)?))
}

#[cfg(windows)]
pub type IpcStream = tokio::net::windows::named_pipe::NamedPipeClient;

#[cfg(unix)]
pub type IpcStream = tokio::net::UnixStream;

#[cfg(windows)]
pub struct IpcListener {
    server: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

#[cfg(unix)]
pub struct IpcListener {
    listener: tokio::net::UnixListener,
}

impl IpcListener {
    #[cfg(windows)]
    pub fn bind(endpoint: &str) -> Result<Self, IpcError> {
        use tokio::net::windows::named_pipe::ServerOptions;

        Ok(Self {
            server: Some(
                ServerOptions::new()
                    .first_pipe_instance(true)
                    .create(endpoint)?,
            ),
        })
    }

    #[cfg(unix)]
    pub fn bind(endpoint: &str) -> Result<Self, IpcError> {
        let path = std::path::Path::new(endpoint);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(Self {
            listener: tokio::net::UnixListener::bind(path)?,
        })
    }

    #[cfg(windows)]
    pub async fn accept(
        mut self,
    ) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, IpcError> {
        let server = self
            .server
            .take()
            .expect("named pipe listener accepts once");
        server.connect().await?;
        Ok(server)
    }

    #[cfg(unix)]
    pub async fn accept(self) -> Result<tokio::net::UnixStream, IpcError> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}

pub async fn connect(endpoint: &str, timeout: Duration) -> Result<IpcStream, IpcError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match connect_once(endpoint).await {
            Ok(stream) => return Ok(stream),
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
            Err(_) => return Err(IpcError::ConnectTimeout(endpoint.to_string())),
        }
    }
}

#[cfg(windows)]
async fn connect_once(endpoint: &str) -> io::Result<IpcStream> {
    tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint)
}

#[cfg(unix)]
async fn connect_once(endpoint: &str) -> io::Result<IpcStream> {
    tokio::net::UnixStream::connect(endpoint).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{HostCommand, HostRequest};

    #[tokio::test]
    async fn frames_round_trip() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let expected = HostWireMessage::Request(HostRequest {
            id: "request-1".into(),
            operation_id: Some("operation-1".into()),
            command: HostCommand::Health,
        });

        write_message(&mut client, &expected)
            .await
            .expect("write frame");
        let received = read_message(&mut server)
            .await
            .expect("read frame")
            .expect("message");
        let json = serde_json::to_value(received).expect("serialize received");
        assert_eq!(json["kind"], "request");
        assert_eq!(json["message"]["id"], "request-1");
    }

    #[tokio::test]
    async fn rejects_oversized_frames() {
        let (mut client, mut server) = tokio::io::duplex(16);
        client
            .write_u32((MAX_FRAME_BYTES + 1) as u32)
            .await
            .expect("write length");
        let error = read_message(&mut server).await.expect_err("reject frame");
        assert!(matches!(error, IpcError::FrameTooLarge(_)));
    }
}
