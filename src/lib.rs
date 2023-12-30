use byteorder::{BigEndian, WriteBytesExt};
use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};

use anyhow::Result;
//use bincode::config::BigEndian;
use bincode::Options;
use serde::{Deserialize, Serialize};
use encoding::all::ISO_8859_1;
use encoding::Encoding;

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemInfo {
    system: GetSysInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetSysInfo {
    get_sysinfo: SysInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SysInfo {
    alias: String,
    child_num: u8,
    children: Vec<Child>,
    #[serde(rename = "deviceId")]
    device_id: String,
    err_code: u8,
    feature: String,
    #[serde(rename = "hwId")]
    hw_id: String,
    hw_ver: String,
    latitude_i: i32,
    led_off: u8,
    longitude_i: i32,
    mac: String,
    mic_type: String,
    model: String,
    #[serde(rename = "oemId")]
    oem_id: String,
    rssi: i8,
    status: String,
    sw_ver: String,
    updating: u8,
}

impl SysInfo {
    pub fn find_child_by_alias(&self, alias: &str) -> Option<&Child> {
        self.children.iter().find(|&child| child.alias == alias)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Child {
    alias: String,
    id: String,
    next_action: NextAction,
    on_time: u32,
    state: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NextAction {
    #[serde(rename = "type")]
    type_field: i8,
}
pub struct SmartPowerStrip {
    ip: String,
    port: u16,
    protocol: Protocol,
    device_id: Option<String>,
    sys_info: Option<SysInfo>,
    timeout: f32,
}

pub enum Protocol {
    Tcp,
    Udp,
}

pub enum PlugState {
    On,
    Off,
}

impl SmartPowerStrip {
    pub fn new(
        ip: String,
        device_id: Option<String>,
        timeout: Option<f32>,
        protocol: Option<Protocol>,
    ) -> Self {
        let timeout = timeout.unwrap_or(2.0);
        let protocol = protocol.unwrap_or(Protocol::Tcp);
        let mut s = SmartPowerStrip {
            ip,
            port: 9999,
            protocol,
            device_id,
            sys_info: None,
            timeout,
        };
        s.sys_info = Some(s.get_system_info().system.get_sysinfo);
        if s.device_id.is_none() {
            s.device_id = s
                .sys_info
                .as_ref()
                .map(|sys_info| sys_info.device_id.clone());
        }
        s
    }

    pub fn get_system_info(&self) -> SystemInfo {
        let result = self.udp_send_command(r#"{"system":{"get_sysinfo":{}}}"#);
        let response: SystemInfo = serde_json::from_str(&result).unwrap();
        response
    }

    fn send_command(&self, command: &str, protocol: &Protocol) -> String {
        match protocol {
            Protocol::Tcp => self.tcp_send_command(command),
            Protocol::Udp => self.udp_send_command(command),
        }
    }

    pub fn toggle_plug(&self, name: &str, state: PlugState) -> Result<()> {
        let state_number = match state {
            PlugState::On => 1,
            PlugState::Off => 0,
        };

        if self.sys_info.is_none() {
            return Err(anyhow::anyhow!(
                "Could not get system info from power strip"
            ));
        }

        if self.device_id.is_none() {
            return Err(anyhow::anyhow!("Could not get device id from power strip"));
        }

        let child = self.sys_info.as_ref().unwrap().find_child_by_alias(name);
        if child.is_none() {
            return Err(anyhow::anyhow!("Could not find child with alias {}", name));
        }

        let plug_id = format!("{}{}", self.device_id.as_ref().unwrap(), child.unwrap().id);
        let relay_command = format!(
            r#"{{"context":{{"child_ids":["{}"]}},"system":{{"set_relay_state":{{"state":{}}}}}}}"#,
            plug_id, state_number
        );
        println!("Sending command: {}", relay_command);
        // self.send_command(&relay_command, &self.protocol);
        self.send_command(&relay_command, &Protocol::Udp);
        Ok(())
    }

    fn tcp_send_command(&self, command: &str) -> String {
        let mut stream = TcpStream::connect((self.ip.as_str(), self.port)).unwrap();
        stream.write_all(command.as_bytes()).unwrap();
        let mut data = String::new();
        stream.read_to_string(&mut data).unwrap();
        data
    }

    fn udp_send_command(&self, command: &str) -> String {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("couldn't bind to address");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs_f32(self.timeout)))
            .expect("set_read_timeout call failed");
        let addr = format!("{}:{}", self.ip, self.port);

        let encrypted_command = self.encrypt_command(command, false);
        socket
            .send_to(&encrypted_command, addr)
            .expect("couldn't send data");

        let mut buf = [0; 2048];
        let (amt, _src) = socket.recv_from(&mut buf).expect("didn't receive data");

        self.decrypt_command(&buf[..amt])
    }

    fn encrypt_command(&self, command: &str, prepend_length: bool) -> Vec<u8> {
        let mut key = 171;
        let mut result = Vec::new();

        if prepend_length {
            let length = command.len() as u32;
            let bytes = bincode::options().with_big_endian().serialize(&length).unwrap();
            result.write_all(&bytes).expect("TODO: panic message");
            //result.write_u32::<BigEndian>(string.len() as u32).unwrap();
        }

        for i in ISO_8859_1.encode(command, encoding::EncoderTrap::Replace).unwrap() {
            let a = key ^ i;
            key = a;
            result.push(a);
        }

        result
    }

    fn decrypt_command(&self, string: &[u8]) -> String {
        let mut key = 171;
        let mut result = Vec::new();

        for &i in string {
            let a = key ^ i;
            key = i;
            result.push(a);
        }

        String::from_utf8(result).unwrap_or_else(|_| String::from("Invalid UTF-8"))
    }

    fn pack<T: Serialize>(data: &str) -> Result<Vec<u8>> {
        //  let bytes = bincode::serialize::<bincode::config::BigEndian>(data)?;
        let bytes = bincode::options().with_big_endian().serialize(data)?;
        Ok(bytes)
    }

}
