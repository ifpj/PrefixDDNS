use netlink_sys::{TokioSocket, SocketAddr, AsyncSocket, AsyncSocketExt};
use netlink_packet_route::{
    RouteNetlinkMessage, 
    AddressFamily,
    address::{AddressMessage, AddressAttribute, AddressHeaderFlag},
};
use netlink_packet_core::{NetlinkMessage, NetlinkPayload};
use std::net::{Ipv6Addr, IpAddr};
use tokio::sync::broadcast;
use anyhow::Result;
use colored::Colorize;
use chrono::Local;

pub struct NetlinkMonitor {
    tx: broadcast::Sender<Ipv6Addr>,
    run_on_startup: bool,
    interface_index: Option<u32>,
}

impl NetlinkMonitor {
    pub fn new(tx: broadcast::Sender<Ipv6Addr>, _run_on_startup: bool, interface_index: Option<u32>) -> Self {
        Self { tx, run_on_startup: true, interface_index } // Force run_on_startup to true
    }

    pub async fn run(&self) -> Result<()> {
        // NETLINK_ROUTE is 0
        let mut socket = TokioSocket::new(0)?;
        
        // RTMGRP_IPV6_IFADDR = 0x100
        let addr = SocketAddr::new(0, 0x100); 
        socket.socket_mut().bind(&addr)?;

        println!("{} {} Netlink monitor started, listening for IPv6 changes...", Local::now().format("%Y-%m-%d %H:%M:%S").to_string().dimmed(), "[Init]".green());

        // If configured, fetch existing addresses immediately
        if self.run_on_startup {
             self.fetch_existing_addresses().await?;
        }

        let mut buf = vec![0u8; 8192];

        loop {
            // recv_from appends to buf
            socket.recv_from(&mut buf).await?;
            let len = buf.len();
            let mut offset = 0;
            
            while offset < len {
                let bytes = &buf[offset..len];
                if bytes.len() < 4 { break; } 

                let msg = match <NetlinkMessage<RouteNetlinkMessage>>::deserialize(bytes) {
                    Ok(m) => m,
                    Err(_) => break,
                };
                
                let msg_len = msg.header.length as usize;
                if msg_len == 0 || msg_len > bytes.len() { break; }

                if let NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewAddress(addr_msg)) = msg.payload {
                    self.process_message(addr_msg);
                }

                offset += msg_len;
            }
            buf.clear();
        }
    }

    pub async fn get_current_ipv6(interface_index: Option<u32>) -> Result<Option<Ipv6Addr>> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let mut links = handle.address().get().execute();
        use futures::stream::TryStreamExt;
        
        while let Some(msg) = links.try_next().await.unwrap_or(None) {
            if let Some(addr) = Self::extract_ipv6_from_message(msg, interface_index) {
                 return Ok(Some(addr));
            }
        }
        Ok(None)
    }

    async fn fetch_existing_addresses(&self) -> Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let mut links = handle.address().get().execute();
        use futures::stream::TryStreamExt;
        
        while let Some(msg) = links.try_next().await.unwrap_or(None) {
            self.process_message(msg);
        }
        Ok(())
    }

    fn process_message(&self, msg: AddressMessage) {
        if let Some(addr) = Self::extract_ipv6_from_message(msg, self.interface_index) {
             let _ = self.tx.send(addr);
        }
    }

    fn extract_ipv6_from_message(msg: AddressMessage, interface_index: Option<u32>) -> Option<Ipv6Addr> {
        if msg.header.family != AddressFamily::Inet6 {
            return None;
        }

        if let Some(index) = interface_index {
            if msg.header.index != index {
                return None;
            }
        }

        // Ignore tentative addresses (Duplicate Address Detection in progress)
        if msg.header.flags.contains(&AddressHeaderFlag::Tentative) {
            return None;
        }

        let mut ipv6_addr = None;

        for attr in msg.attributes {
            if let AddressAttribute::Address(IpAddr::V6(addr)) = attr {
                ipv6_addr = Some(addr);
            }
        }

        if let Some(addr) = ipv6_addr {
            if addr.is_loopback() || addr.is_multicast() || (addr.segments()[0] & 0xffc0) == 0xfe80 {
                return None;
            }
            return Some(addr);
        }
        None
    }
}

