use crate::lib::{ClientRequest, ClientResponse, MemStore};
use crate::network::TonicgRPCNetwork;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};

use raft_proto::raft_rpc_server::{RaftRpc, RaftRpcServer};
use raft_proto::{
  AppendEntriesRpcReply, AppendEntriesRpcRequest, ClientReadRpcReply, ClientReadRpcRequest, ClientWriteRpcReply, ClientWriteRpcRequest,
  InstallSnapshotRpcReply, InstallSnapshotRpcRequest, VoteRequestRpcReply, VoteRequestRpcRequest,
};
use std::sync::Arc;

use async_raft::raft::{AppendEntriesRequest, InstallSnapshotRequest, InstallSnapshotResponse, VoteRequest};
use async_raft::raft::{ClientWriteRequest, Raft};

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
  async fn append_entries(&self, request: Request<AppendEntriesRpcRequest>) -> Result<Response<AppendEntriesRpcReply>, Status> {
    let serialized = request.into_inner().data;
    let deserialized: AppendEntriesRequest<ClientRequest> = serde_json::from_str(&serialized).unwrap();

    let entries = deserialized.entries.clone();

    if entries.len() > 0 {
      println!("Got a append_entries request: {:?}", deserialized);
    }

    let response = self.raft.append_entries(deserialized).await.unwrap();
    let reply = AppendEntriesRpcReply { data: serde_json::to_string(&response).unwrap() };

    Ok(Response::new(reply))
  }

  async fn vote_request(&self, request: Request<VoteRequestRpcRequest>) -> Result<Response<VoteRequestRpcReply>, Status> {
    let serialized = request.into_inner().data;
    let deserialized: VoteRequest = serde_json::from_str(&serialized).unwrap();

    println!("Got a vote request: {:?}", deserialized);

    let response = self.raft.vote(deserialized).await.unwrap();
    let reply = VoteRequestRpcReply { data: serde_json::to_string(&response).unwrap() };

    Ok(Response::new(reply))
  }

  async fn install_snapshot(&self, request: Request<InstallSnapshotRpcRequest>) -> Result<Response<InstallSnapshotRpcReply>, Status> {
    let serialized = request.into_inner().data;
    let deserialized: InstallSnapshotRequest = serde_json::from_str(&serialized).unwrap();

    println!("Got a install_snapshot request: {:?}", deserialized);

    let response = self.raft.install_snapshot(deserialized).await.unwrap();
    let reply = InstallSnapshotRpcReply { data: serde_json::to_string(&response).unwrap() };

    Ok(Response::new(reply))
  }

  async fn client_write(&self, request: Request<ClientWriteRpcRequest>) -> Result<Response<ClientWriteRpcReply>, Status> {
    let new_log = ClientRequest {
      client: "0".into(),
      serial: 0,
      status: request.into_inner().status,
    };

    println!("Got a client_write request: {:?}", new_log.clone());

    let raft_request = ClientWriteRequest::new(new_log);
    let raft_response = self.raft.client_write(raft_request).await.unwrap();

    let reply = ClientWriteRpcReply { status: "success".into() };
    Ok(Response::new(reply))
  }

  async fn client_read(&self, request: Request<ClientReadRpcRequest>) -> Result<Response<ClientReadRpcReply>, Status> {
    let state_machine = self.storage.read_state_machine().await;

    println!("{:?} ", state_machine);
    let reply = ClientReadRpcReply {
      status: serde_json::to_string(&state_machine.client_status).unwrap(),
    };
    Ok(Response::new(reply))
  }
}

pub async fn start_server(raft: MyRaft, storage: Arc<MemStore>, address: String) {
  let addr = address.parse().unwrap();
  let raft_server = RaftRpcService::new(raft, storage);
  Server::builder()
    .add_service(RaftRpcServer::new(raft_server))
    .serve(addr)
    .await;
}
