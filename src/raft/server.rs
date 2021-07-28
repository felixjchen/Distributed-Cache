use crate::raft::network::TonicgRPCNetwork;
use crate::raft::storage::{ClientRequest, ClientResponse, MemStore};
use anyhow::Result;
use async_raft::error::{ClientReadError, ClientWriteError};
use async_raft::raft::{AppendEntriesRequest, InstallSnapshotRequest, VoteRequest};
use async_raft::raft::{ClientWriteRequest, Raft};
use raft_proto::raft_rpc_server::{RaftRpc, RaftRpcServer};
use raft_proto::{
  AppendEntriesRpcReply, AppendEntriesRpcRequest, ClientReadRpcReply, ClientReadRpcRequest,
  ClientWriteRpcReply, ClientWriteRpcRequest, InstallSnapshotRpcReply, InstallSnapshotRpcRequest,
  VoteRequestRpcReply, VoteRequestRpcRequest,
};
use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};

pub mod raft_proto {
  tonic::include_proto!("raft_proto");
}

pub type MyRaft = Raft<ClientRequest, ClientResponse, TonicgRPCNetwork, MemStore>;

pub struct RaftRpcService {
  raft: MyRaft,
  storage: Arc<MemStore>,
}

impl RaftRpcService {
  pub fn new(raft: MyRaft, storage: Arc<MemStore>) -> RaftRpcService {
    RaftRpcService { raft, storage }
  }
}

#[tonic::async_trait]
impl RaftRpc for RaftRpcService {
  async fn append_entries(
    &self,
    request: Request<AppendEntriesRpcRequest>,
  ) -> Result<Response<AppendEntriesRpcReply>, Status> {
    let serialized = request.into_inner().data;
    let deserialized: AppendEntriesRequest<ClientRequest> =
      serde_json::from_str(&serialized).unwrap();

    let entries = deserialized.entries.clone();

    println!("Got a append_entries request: {:?}", deserialized);

    let response = self.raft.append_entries(deserialized).await.unwrap();
    let reply = AppendEntriesRpcReply {
      data: serde_json::to_string(&response).unwrap(),
    };

    Ok(Response::new(reply))
  }

  async fn vote_request(
    &self,
    request: Request<VoteRequestRpcRequest>,
  ) -> Result<Response<VoteRequestRpcReply>, Status> {
    let serialized = request.into_inner().data;
    let deserialized: VoteRequest = serde_json::from_str(&serialized).unwrap();

    let response = self.raft.vote(deserialized).await.unwrap();
    let reply = VoteRequestRpcReply {
      data: serde_json::to_string(&response).unwrap(),
    };

    Ok(Response::new(reply))
  }

  async fn install_snapshot(
    &self,
    request: Request<InstallSnapshotRpcRequest>,
  ) -> Result<Response<InstallSnapshotRpcReply>, Status> {
    let serialized = request.into_inner().data;
    let deserialized: InstallSnapshotRequest = serde_json::from_str(&serialized).unwrap();

    let response = self.raft.install_snapshot(deserialized).await.unwrap();
    let reply = InstallSnapshotRpcReply {
      data: serde_json::to_string(&response).unwrap(),
    };

    Ok(Response::new(reply))
  }

  async fn client_write(
    &self,
    request: Request<ClientWriteRpcRequest>,
  ) -> Result<Response<ClientWriteRpcReply>, Status> {
    let request = request.into_inner();
    let key = request.key;
    let value = request.value;

    let new_log = ClientRequest { key, value };
    let raft_request = ClientWriteRequest::new(new_log);
    let reply = match self.raft.client_write(raft_request).await {
      Ok(_) => ClientWriteRpcReply { leader_id: None },
      Err(ClientWriteError::ForwardToLeader(_, leader_id)) => ClientWriteRpcReply { leader_id },
      Err(_) => panic!("raft write error"),
    };

    Ok(Response::new(reply))
  }

  async fn client_read(
    &self,
    request: Request<ClientReadRpcRequest>,
  ) -> Result<Response<ClientReadRpcReply>, Status> {
    let request = request.into_inner();
    let key = request.key;

    // Guard stale reads
    let state_machine = self.storage.read_state_machine().await;
    let reply = match self.raft.client_read().await {
      Ok(_) => ClientReadRpcReply {
        value: state_machine.kv_store.get(&key).cloned(),
        leader_id: None,
      },
      Err(ClientReadError::ForwardToLeader(leader_id)) => ClientReadRpcReply {
        value: None,
        leader_id,
      },
      Err(_) => panic!("raft write error"),
    };
    Ok(Response::new(reply))
  }
}

pub async fn start_server(
  raft: MyRaft,
  storage: Arc<MemStore>,
  address: String,
) -> Result<(), Box<dyn std::error::Error>> {
  let addr = address.parse().unwrap();
  let service = RaftRpcService::new(raft, storage);
  Server::builder()
    .add_service(RaftRpcServer::new(service))
    .serve(addr)
    .await?;
  Ok(())
}
