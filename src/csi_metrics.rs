use chrono::NaiveDateTime;
use eui48::MacAddress;
use ndarray::{Array2, Axis, s};
use ringbuffer::RingBuffer;
use sci_rs::signal;
use staged_sg_filter::sav_gol_f32;

// use ndarray_linalg::Eig; // for eigen decomposition

#[derive(Clone, Debug)]
pub struct CSIMetricsPCCEntry {
    pub sensor_id: MacAddress,
    pub timestamp: NaiveDateTime,
    pub level: i16, // 1 = 10ms, 2 = 100ms

    pub correlation_coefficient: f32,
}

const SAVGOL_WINDOW_SIZE: usize = 25;

use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Q_BUTTERWORTH_F32};

fn apply_bandpass_filter(data: &[f32], fs: f32, low_cut: f32, high_cut: f32) -> Vec<f32> {
    let center_freq = (low_cut + high_cut) / 2.0;
    let bandwidth = high_cut - low_cut;
    let q = center_freq / bandwidth;

    let coeffs = Coefficients::<f32>::from_params(
        biquad::Type::BandPass,
        fs.hz(),
        center_freq.hz(),
        q
    ).unwrap();

    let mut filter = DirectForm1::<f32>::new(coeffs);
    data.iter().map(|&x| filter.run(x)).collect()
}

// fn pca(data: &Array2<f32>, n_components: usize) -> (Array2<f32>, Array2<f32>) {
//     // Step 1: Center data (mean subtraction)
//     let mean = data.mean_axis(Axis(0)).unwrap();
//     let centered = data - &mean;
//
//     // Step 2: Covariance matrix (features x features)
//     // Cov = (centered.T dot centered) / (n_samples - 1)
//     let n_samples = data.shape()[0] as f32;
//     let cov = centered.t().dot(&centered) / (n_samples - 1.0);
//
//     // Step 3: Eigen decomposition of covariance matrix
//     let (eigenvalues, eigenvectors) = cov.eig().expect("Eigen decomposition failed");
//
//     let eigenvalues = eigenvalues.mapv(|c| c.re);
//     let eigenvectors = eigenvectors.mapv(|c| c.re);
//
//     // Step 4: Sort eigenvalues & eigenvectors descending
//     let mut eig_pairs: Vec<(f32, ndarray::ArrayView1<f32>)> =
//         eigenvalues.iter().zip(eigenvectors.axis_iter(Axis(1))).map(|(&val, vec)| (val, vec)).collect();
//
//     eig_pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
//
//     // Select top n_components
//     let components_vec: Vec<_> = eig_pairs[..n_components]
//         .iter()
//         .map(|(_, v)| v.to_owned())
//         .collect();
//
//     let views: Vec<_> = components_vec.iter().map(|arr| arr.view()).collect();
//     let components = ndarray::stack(Axis(1), &views).unwrap();
//
//     // Step 5: Project data onto components
//     let transformed = centered.dot(&components);
//
//     (transformed, components)
// }

pub fn compute_pcc_from_buffer(matrix: &Array2<f32>, target_rate: usize, target_samples: usize) -> Option<Vec<f32>> {
    // Step 1: Compute variance
    let variances = matrix.var_axis(Axis(0), 0.0);
    let variance_threshold = 1e-4;

    // Step 2: Identify subcarriers below the variance threshold
    let selected_indices: Vec<usize> = variances.iter()
        .enumerate()
        .filter(|&(_, &var)| var > variance_threshold)
        .map(|(i, _)| i)
        .collect();

    if selected_indices.is_empty() {
        return None;
    }

    // Step 3: Stack selected subcarriers into new matrix
    let views: Vec<_> = selected_indices.iter()
        .map(|&i| matrix.slice(s![.., i]).insert_axis(Axis(1)))
        .collect();
    let selected_matrix = ndarray::stack(Axis(1), &views).ok()?;

    let n_components = 8;

    // Step 4: Resample/Filter/Downsample
    let mut output = Array2::<f32>::zeros((target_samples, selected_indices.len()));
    for (i, col) in selected_matrix.axis_iter(Axis(1)).enumerate() {
        let col_1d = col.to_owned().into_shape(col.len()).ok()?;

        // Resample to expected number of samples
        let resampled = signal::resample::resample(&col_1d.to_vec(), target_rate);
        let mut filtered_output = resampled.clone();

        // let bandpassed = apply_bandpass_filter(&resampled.as_slice(), 100.0, 0.5, 5.0);

        // Apply Savitzky Golay Filter
        sav_gol_f32::<12, 3>(&mut filtered_output, &resampled);

        // downsample to target sample count
        let downsampled = signal::resample::resample(&filtered_output, target_samples);

        output.slice_mut(s![.., i]).assign(&ndarray::Array1::from(downsampled));
    }

    // let (transformed_output, components) = pca(&output, n_components);

    // Step 5: Return correlation for each frame in the filtered signal
    let mut pccs = Vec::new();
    for i in 0..(target_samples - 1) {
        pccs.push(crate::csi::get_correlation_coefficient(
            output.row(i).to_vec(),
            &output.row(i + 1).to_vec(),
        ))
    }

    Some(pccs)
}
