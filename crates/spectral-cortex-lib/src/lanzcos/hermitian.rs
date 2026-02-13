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
    ///
    /// # Arguments
    ///
    /// * `iterations` - Number of iterations of the Lanczos algorithm
    /// * `order` - Sort in ascending (Smallest) or Descending (Largest) order
    ///   of the Eigen values
    ///
    ///  # Example
    ///
    ///  ```
    /// # use nalgebra::DMatrix;
    /// # use spectral_cortex::lanzcos::{Hermitian, Order};
    /// let matrix = DMatrix::<f64>::from_fn(100, 100, |_, _| rand::random::<f64>());
    /// let eigen = matrix.eigsh(50, Order::Smallest);
    ///
    /// let eigenval = eigen.eigenvalues[0];
    /// let eigenvec = eigen.eigenvectors.column(0);
    ///  ```
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

// Sparse matrix implementations removed due to nalgebra version compatibility issues
// We only need DMatrix for our use case
