use crate::utils::Config;
use ads1x1x::channel::{DifferentialA0A1, DifferentialA2A3};
use ads1x1x::ic::{Ads1115, Resolution16Bit};
use ads1x1x::mode::Continuous;
use ads1x1x::{Ads1x1x, FullScaleRange, TargetAddr};
use anyhow::{anyhow, Result};
use ftdi_embedded_hal::FtHal;
use ftdi_embedded_hal::{
    libftd2xx::{self, Ft232h},
    I2c,
};

pub type Adc = Ads1x1x<I2c<Ft232h>, Ads1115, Resolution16Bit, Continuous>;

pub trait AdcBackend {
    fn read_voltage(&mut self) -> Result<[f64; 2]>;
}

pub struct AdcModule {
    adc1: Adc,
    adc2: Adc,
}

impl AdcModule {
    pub async fn new(config: &Config) -> Result<Self> {
        let [adc1, adc2] = init_adcs().await?;
        return Ok(AdcModule { adc1, adc2 });
    }
}

impl AdcBackend for AdcModule {
    fn read_voltage(&mut self) -> Result<[f64; 2]> {
        let (vol1, vol2) = rayon::join(
            || read_voltage(&mut self.adc1),
            || read_voltage(&mut self.adc2),
        );
        let vol1 = vol1?;
        let vol2 = vol2?;
        return Ok([vol1, vol2]);
    }
}

pub struct MockAdcModule {}

impl MockAdcModule {
    pub async fn new(config: &Config) -> Result<Self> {
        return Ok(MockAdcModule {});
    }
}

impl AdcBackend for MockAdcModule {
    fn read_voltage(&mut self) -> Result<[f64; 2]> {
        return Ok([0.; 2]);
    }
}

pub async fn init_adcs() -> Result<[Adc; 2]> {
    tracing::debug!("initializing adcs");
    match libftd2xx::num_devices()? {
        0..2 => {
            return Err(anyhow!(
                "Too few adc modules connected! Make sure two are plugged in."
            ))
        }
        2 => (),
        3.. => {
            return Err(anyhow!(
                "Too many adc modules connected! Make sure two are plugged in."
            ))
        }
    };

    let adcs: [Result<(Adc, u8)>; 2] = [0, 1].map(|i| {
        let device = libftd2xx::Ftdi::with_index(i)?;
        let device = libftd2xx::Ft232h::try_from(device)?;
        let hal = FtHal::init_freq(device, 400_000)?;
        let Ok(i2c) = hal.i2c() else {
            return Err(anyhow!("Failed to create I2C device"));
        };
        let adc = Ads1x1x::new_ads1115(i2c, TargetAddr::default());

        let Ok(adc) = adc.into_continuous() else {
            return Err(anyhow!("Failed set ADC continuous mode"));
        };
        let Ok(mut adc) = adc.into_one_shot() else {
            return Err(anyhow!("Failed set ADC one shot mode"));
        };

        let Ok(val) = nb::block!(adc.read(DifferentialA2A3)) else {
            return Err(anyhow!("Failed to read index voltage"));
        };

        let idx = match val {
            ..10 => 1,
            10.. => 2,
        };

        tracing::debug!("adc index value: {}", val);

        let Ok(mut adc) = adc.into_continuous() else {
            return Err(anyhow!("Failed set ADC continuous mode"));
        };
        let Ok(_) = adc.select_channel(DifferentialA0A1) else {
            return Err(anyhow!("Failed to set channel to differentialA0A1"));
        };
        let Ok(_) = adc.set_full_scale_range(FullScaleRange::Within4_096V) else {
            return Err(anyhow!("Failed set ADC range"));
        };

        return Ok((adc, idx));
    });

    let [adc1, adc2] = adcs;
    let (adc1, idx1) = adc1?;
    let (adc2, idx2) = adc2?;

    return Ok(match [idx1, idx2] {
        [1, 2] => [adc1, adc2],
        [2, 1] => [adc2, adc1],
        _ => Err(anyhow!("Invalid adc configuration"))?,
    });
}

pub async fn get_adc_module(config: &Config) -> Result<Box<dyn AdcBackend + Send>> {
    match config.mock_adc {
        true => return Ok(Box::new(MockAdcModule::new(config).await?)),
        false => return Ok(Box::new(AdcModule::new(config).await?)),
    }
}

fn read_voltage(adc: &mut Adc) -> Result<f64> {
    let Ok(raw) = adc.read() else {
        return Err(anyhow!("Failed to read from ADC"));
    };
    let voltage = raw as f64 * 4.069 / 32767.;

    tracing::debug!("voltage read {}", voltage);

    Ok(voltage)
}
