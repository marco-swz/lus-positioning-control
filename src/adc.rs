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
    pub fn new() -> Result<Self> {
        let [adc1, adc2] = init_adcs()?;
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

pub struct MockAdcModule {
    fn_read: Box<dyn Fn() -> Result<[f64; 2]> + Send>,
}

impl MockAdcModule {
    pub fn new(fn_read: Box<dyn Fn() -> Result<[f64; 2]> + Send>) -> Result<Self> {
        return Ok(MockAdcModule { fn_read });
    }
}

impl AdcBackend for MockAdcModule {
    fn read_voltage(&mut self) -> Result<[f64; 2]> {
        return (self.fn_read)();
    }
}

fn init_adcs() -> Result<[Adc; 2]> {
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
        let i2c = hal
            .i2c()
            .map_err(|e| anyhow!("Failed to create I2C device: {:?}", e))?;
        let adc = Ads1x1x::new_ads1115(i2c, TargetAddr::default());

        let mut adc = adc
            .into_continuous()
            .map_err(|_e| anyhow!("Failed set ADC continuous mode: TimeoutError"))?;

        adc.select_channel(DifferentialA2A3)
            .map_err(|e| anyhow!("Failed to set channel to differentialA2A3: {:?}", e))?;

        // The current conversion must finish, before the channel change is in effect.
        std::thread::sleep(std::time::Duration::from_millis(1000));

        let val = adc
            .read()
            .map_err(|e| anyhow!("Failed to read index voltage: {:?}", e))?;

        dbg!(i, val);
        let idx = match val {
            ..10 => 1,
            10.. => 2,
        };

        tracing::debug!("adc index value: {}", val);

        adc.select_channel(DifferentialA0A1)
            .map_err(|e| anyhow!("Failed to set channel to differentialA0A1: {:?}", e))?;

        // The current conversion must finish, before the channel change is in effect.
        std::thread::sleep(std::time::Duration::from_millis(1000));

        adc.set_full_scale_range(FullScaleRange::Within4_096V)
            .map_err(|e| anyhow!("Failed set ADC range: {:?}", e))?;

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

pub fn get_adc_module(config: &Config) -> Result<Box<dyn AdcBackend + Send>> {
    match config.mock_adc {
        true => return Ok(Box::new(MockAdcModule::new(Box::new(|| Ok([0.; 2])))?)),
        false => return Ok(Box::new(AdcModule::new()?)),
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
