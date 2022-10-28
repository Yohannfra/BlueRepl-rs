use super::{
    BleController, BlePeripheral, BlePeripheralInfo, Characteristic, CharacteristicProperties,
    Service,
};
use async_trait::async_trait;
use std::error::Error;
use std::time::Duration;
use tokio::time;

// mod utils;
use crate::utils;

use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager};

pub struct BtleplugController {
    connected: bool,
    adapter: Adapter,
    scan_list: Vec<BlePeripheral>,
}

#[async_trait]
impl BleController for BtleplugController {
    async fn scan(&mut self, scan_time_s: usize) -> Result<(), Box<dyn Error>> {
        println!("Scanning for {} seconds...", scan_time_s);

        self.adapter.start_scan(ScanFilter::default()).await?; // start scan
        time::sleep(Duration::from_secs(scan_time_s as u64)).await; // wait x seconds
        self.adapter.stop_scan().await?; // stop scan

        let peripherals = self.adapter.peripherals().await?;
        let mut periph_vec: Vec<BlePeripheral> = Vec::new();

        for (index, p) in peripherals.into_iter().enumerate() {
            let properties = p.properties().await?.unwrap();
            let name = properties
                .local_name
                .unwrap_or_else(|| String::from("unknown"));
            let mut company_code = std::usize::MAX;
            if let Some((code, _)) = properties.manufacturer_data.iter().next() {
                company_code = *code as usize;
            }

            let rssi: i16 = properties.rssi.unwrap_or(0);

            periph_vec.push(BlePeripheral {
                name,
                address_uuid: self.get_address_or_uuid(&p).await?,
                rssi,
                id: index,
                company_id: company_code,
            });
        }
        self.scan_list = periph_vec;
        Ok(())
    }

    fn get_scan_list(&self) -> Vec<BlePeripheral> {
        self.scan_list.clone()
    }

    async fn get_adapter_infos(&self) -> Result<String, Box<dyn Error>> {
        let adapter_infos: String = self.adapter.adapter_info().await?;
        Ok(adapter_infos)
    }

    async fn write(
        &mut self,
        _service: &str,
        characteristic: &str,
        payload: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        let mut char_found = false;

        for p in &self.adapter.peripherals().await? {
            if p.is_connected().await? {
                p.discover_services().await.unwrap();
                for c in p.characteristics() {
                    if c.uuid.to_string() == characteristic {
                        println!("Writing {:?} to characteristic {}", payload, c.uuid);
                        char_found = true;
                        p.write(&c, payload, btleplug::api::WriteType::WithoutResponse)
                            .await?;
                    }
                }
            }
        }

        if !char_found {
            Err(format!("Characteristic: {} not found", characteristic))?
        }

        Ok(())
    }

    async fn read(&mut self, _service: &str, characteristic: &str) -> Result<(), Box<dyn Error>> {
        let mut char_found = false;

        for p in &self.adapter.peripherals().await? {
            if p.is_connected().await? {
                p.discover_services().await.unwrap();
                for c in p.characteristics() {
                    if c.uuid.to_string() == characteristic {
                        char_found = true;
                        println!("Reading Characteristic {} ...", c.uuid);
                        let content = p.read(&c).await?;
                        println!("{:?}", content);
                    }
                }
            }
        }

        if !char_found {
            Err(format!("Characteristic: {} not found", characteristic))?
        }

        Ok(())
    }

    async fn get_peripheral_infos(&self) -> Result<BlePeripheralInfo, Box<dyn Error>> {
        for p in &self.adapter.peripherals().await? {
            if p.is_connected().await? {
                p.discover_services().await.unwrap();

                let services = p.services();
                let properties = p.properties().await?.unwrap();

                let mut infos = BlePeripheralInfo {
                    periph_name: properties
                        .local_name
                        .unwrap_or_else(|| String::from("unknown")),
                    periph_mac: self.get_address_or_uuid(&p).await?,
                    rssi: properties.rssi.unwrap_or(0),
                    services: Vec::new(),
                };

                for s in services {
                    let mut ser = Service {
                        uuid: s.uuid.to_string(),
                        characteriscics: Vec::new(),
                    };

                    for c in s.characteristics {
                        let mut car = Characteristic {
                            uuid: c.uuid.to_string(),
                            properties: CharacteristicProperties::UNKNOWN,
                        };

                        if c.properties.contains(btleplug::api::CharPropFlags::WRITE) {
                            car.properties |= CharacteristicProperties::WRITE;
                        }

                        if c.properties.contains(btleplug::api::CharPropFlags::READ) {
                            car.properties |= CharacteristicProperties::READ;
                        }

                        if c.properties
                            .contains(btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE)
                        {
                            car.properties |= CharacteristicProperties::WRITE_WITHOUT_RESPONSE;
                        }

                        if c.properties.contains(btleplug::api::CharPropFlags::NOTIFY) {
                            car.properties |= CharacteristicProperties::NOTIFY;
                        }

                        if c.properties
                            .contains(btleplug::api::CharPropFlags::INDICATE)
                        {
                            car.properties |= CharacteristicProperties::INDICATE;
                        }

                        ser.characteriscics.push(car);
                    }
                    infos.services.push(ser);
                }
                return Ok(infos);
            }
        }
        panic!("Code should not arrive here");
    }

    async fn connect(&mut self, uuid: &str) -> Result<(), Box<dyn Error>> {
        for p in &self.adapter.peripherals().await? {
            let properties = p.properties().await?.unwrap();
            let name = properties
                .local_name
                .unwrap_or_else(|| String::from("unknown"));

            if uuid == self.get_address_or_uuid(&p).await? {
                println!(
                    "Connecting to {} with uuid: {}",
                    name,
                    self.get_address_or_uuid(&p).await?
                );
                match p.connect().await {
                    Ok(()) => {
                        self.connected = true;
                        return Ok(());
                    }
                    Err(e) => return Err(format!("{}", e))?,
                }
            }
        }
        Err(format!("Peripheral with uuid {} not found", uuid))?
    }

    async fn disconnect(&mut self) -> Result<(), Box<dyn Error>> {
        for p in &self.adapter.peripherals().await? {
            if p.is_connected().await? {
                let properties = p.properties().await?.unwrap();
                let name = properties
                    .local_name
                    .unwrap_or_else(|| String::from("unknown"));
                println!(
                    "Disconnecting from {} with uuid: {} ... ",
                    name,
                    self.get_address_or_uuid(&p).await?
                );
                self.connected = false;
                p.disconnect().await?;
                break;
            }
        }
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

impl BtleplugController {
    pub async fn new() -> BtleplugController {
        let manager = match Manager::new().await {
            Ok(m) => m,
            Err(e) => panic!("{:?}", e),
        };

        let adapter_list = match manager.adapters().await {
            Ok(v) => v,
            Err(e) => panic!("{:?}", e),
        };

        let adapter = match adapter_list.len() {
            0 => panic!("No adapter available"),
            1 => &adapter_list[0],
            _ => {
                println!("Found multiple adapters, select the one to use:");
                for (index, ad) in adapter_list.iter().enumerate() {
                    println!("[{}]: {:?}", index, ad);
                }
                let n = utils::get_usize_input(">>");
                &adapter_list[n]
            }
        };

        println!(
            "Using BLE adapter: {:?}",
            adapter.adapter_info().await.unwrap()
        );

        BtleplugController {
            connected: false,
            adapter: adapter.clone(),
            scan_list: Vec::new(),
        }
    }

    async fn get_address_or_uuid(
        &self,
        p: &btleplug::platform::Peripheral,
    ) -> Result<String, Box<dyn Error>> {
        let properties = p.properties().await?.unwrap();

        if cfg!(target_os = "macos") {
            return Ok(p.id().to_string());
        } else {
            return Ok(properties.address.to_string());
        }
    }
}
