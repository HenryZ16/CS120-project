#[cfg(test)]
pub mod test_symrs;

// This test is time consuming, so it is disabled by default.
#[cfg(test)]
pub mod test_asio_stream;

#[cfg(test)]
pub mod test_acoustic_modem;