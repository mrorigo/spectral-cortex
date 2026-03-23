use nalgebra_sparse::CsrMatrix;
use nalgebra::{ComplexField, DMatrix, DVector, DVectorView, RealField};

use crate::lanzcos::{HermitianEigen, Order};

pub trait Hermitian<T>: Sized
where
    T: ComplexField + Copy,
    T::RealField: num::Float,
{
    fn nrows(&self) -> usize;
    fn ncols(&self) -> usize;
    fn vector_product(&self, v: DVectorView<T>) -> DVector<T>;

    fn is_square(&self) -> bool {
        self.nrows() == self.ncols()
    }

    /// Computes the Eigen decomposition of an Hermitian matrix
    fn eigsh(&self, iterations: usize, order: Order) -> HermitianEigen<T> {
        HermitianEigen::<T>::new(self, iterations, order, RealField::min_value().unwrap())
    }
}

impl<T> Hermitian<T> for DMatrix<T>
where
    T: ComplexField + Copy,
    T::RealField: num::Float,
{
    fn nrows(&self) -> usize {
        self.nrows()
    }

    fn ncols(&self) -> usize {
        self.ncols()
    }

    fn vector_product(&self, v: DVectorView<T>) -> DVector<T> {
        self * v
    }
}

/// A wrapper for $L = I - D^{-1/2} W D^{-1/2}$ that implements the `Hermitian` trait
/// while storing only the sparse normalized adjacency $W_{norm} = D^{-1/2} W D^{-1/2}$.
/// This avoids the $\mathcal{O}(N)$ memory overhead of storing a diagonal identity matrix in CSR format.
pub struct SparseNormalizedLaplacian<T>
where
    T: ComplexField + Copy,
    T::RealField: num::Float,
{
    pub w_norm: CsrMatrix<T>,
}

impl Hermitian<f32> for SparseNormalizedLaplacian<f32> {
    fn nrows(&self) -> usize {
        self.w_norm.nrows()
    }

    fn ncols(&self) -> usize {
        self.w_norm.ncols()
    }

    fn vector_product(&self, v: DVectorView<f32>) -> DVector<f32> {
        let n = self.w_norm.nrows();
        let mut wv = DVector::zeros(n);
        let v_owned = v.into_owned();
        
        // Manual spmv for reliability across nalgebra-sparse versions
        let row_offsets = self.w_norm.row_offsets();
        let col_indices = self.w_norm.col_indices();
        let values = self.w_norm.values();
        
        for i in 0..n {
            let start = row_offsets[i];
            let end = row_offsets[i+1];
            let mut sum = 0.0f32;
            for k in start..end {
                sum += values[k] * v_owned[col_indices[k]];
            }
            wv[i] = sum;
        }

        // Compute (I - X)v = v - Xv
        v_owned - wv
    }
}
