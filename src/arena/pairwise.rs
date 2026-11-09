/// Result of the weighted least-squares fit, see the module docs.
pub struct Ratings {
    /// Strengths in payoff units, summing to zero.
    pub s: Vec<f64>,
    /// `Cov(s) = L^+`.
    pub cov: Vec<Vec<f64>>,
    pub chi2: f64,
    pub df: isize,
}

impl Ratings {
    /// Standard error of `s_i`, relative to the field average: `sqrt(Cov_ii)`.
    pub fn se(&self, i: usize) -> f64 {
        self.cov[i][i].max(0.0).sqrt()
    }

    /// Standard error of `s_i - s_j`: `sqrt(Cov_ii + Cov_jj - 2 Cov_ij)`.
    pub fn diff_se(&self, i: usize, j: usize) -> f64 {
        (self.cov[i][i] + self.cov[j][j] - 2.0 * self.cov[i][j])
            .max(0.0)
            .sqrt()
    }
}

pub struct PairwiseStats {
    /// samples
    n: Vec<Vec<f64>>,
    /// mean payoff
    mu: Vec<Vec<f64>>,
    /// sum of squared diff
    m2: Vec<Vec<f64>>,
}

impl PairwiseStats {
    pub fn new(m: usize) -> Self {
        Self {
            n: vec![vec![0.0; m]; m],
            mu: vec![vec![0.0; m]; m],
            m2: vec![vec![0.0; m]; m],
        }
    }

    pub fn len(&self) -> usize {
        self.n.len()
    }

    pub fn count(&self, i: usize, j: usize) -> f64 {
        self.n[i][j]
    }

    pub fn mean(&self, i: usize, j: usize) -> f64 {
        self.mu[i][j]
    }

    pub fn total(&self) -> f64 {
        let m = self.len();
        (0..m)
            .flat_map(|i| ((i + 1)..m).map(move |j| (i, j)))
            .map(|(i, j)| self.n[i][j])
            .sum()
    }

    /// Welford's update:
    /// n  <- n + 1
    /// d  =  x - mu
    /// mu <- mu + d / n
    /// m2 <- m2 + d (x - mu_new)
    pub fn record(&mut self, i: usize, j: usize, x: f64) {
        assert!(i != j);

        self.n[i][j] += 1.0;
        let n = self.n[i][j];
        let delta = x - self.mu[i][j];
        self.mu[i][j] += delta / n;
        self.m2[i][j] += delta * (x - self.mu[i][j]);

        // flip
        self.n[j][i] = n;
        self.mu[j][i] = -self.mu[i][j];
        self.m2[j][i] = self.m2[i][j];
    }

    /// `Var(mu_ij) = s2_ij / n_ij`, or `None` if it can't be estimated yet
    pub fn mean_variance(&self, i: usize, j: usize) -> Option<f64> {
        let n = self.n[i][j];
        if n < 2.0 {
            return None;
        }

        let s2 = self.m2[i][j] / (n - 1.0);
        if s2 <= 0.0 {
            return None;
        }

        Some(s2 / n)
    }

    /// Inverse-variance weight `w_ij = n_ij / s2_ij`. Pairs without a variance estimate get 0,
    fn weight(&self, i: usize, j: usize) -> f64 {
        self.mean_variance(i, j).map_or(0.0, |v| 1.0 / v)
    }

    /// Solves `L s = b` with the gauge `sum s = 0`
    /// `L` and `b` are built entrywise: `L_ii = sum_j w_ij`, `L_ij = -w_ij`, `b_i = sum_j w_ij mu_ij`.
    /// Returns `None` if the graph of weighted pairs is disconnected (`L + J` singular).
    pub fn fit(&self) -> Option<Ratings> {
        let m = self.len();
        if m == 0 {
            return None;
        }

        // normal equations: L = A^T W A (weighted Laplacian), b = A^T W mu
        let mut l = vec![vec![0.0; m]; m];
        let mut b = vec![0.0; m];
        for i in 0..m {
            for j in 0..m {
                if i == j {
                    continue;
                }
                let w = self.weight(i, j);
                l[i][i] += w;
                l[i][j] -= w;
                b[i] += w * self.mu[i][j];
            }
        }

        // L^+ = (L + J)^{-1} - J, with J = 1 1^T / m
        let j_entry = 1.0 / m as f64;
        let shifted = l
            .iter()
            .map(|row| row.iter().map(|x| x + j_entry).collect())
            .collect();
        let shifted_inv = invert(shifted)?;

        // s = (L + J)^{-1} b, valid because J b = 0
        let s: Vec<f64> = shifted_inv
            .iter()
            .map(|row| row.iter().zip(&b).map(|(x, y)| x * y).sum())
            .collect();

        let cov: Vec<Vec<f64>> = shifted_inv
            .iter()
            .map(|row| row.iter().map(|x| x - j_entry).collect())
            .collect();

        // weighted residuals
        let mut chi2 = 0.0;
        let mut pairs = 0isize;
        for i in 0..m {
            for j in (i + 1)..m {
                let w = self.weight(i, j);
                if w == 0.0 {
                    continue;
                }
                let r = self.mu[i][j] - (s[i] - s[j]);
                chi2 += w * r * r;
                pairs += 1;
            }
        }

        Some(Ratings {
            s,
            cov,
            chi2,
            df: pairs - (m as isize - 1),
        })
    }
}

/// Inverts a square matrix by Gauss-Jordan elimination with partial pivoting.
/// Returns `None` if the matrix is (numerically) singular.
fn invert(mut a: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let m = a.len();
    let mut inv: Vec<Vec<f64>> = (0..m)
        .map(|i| (0..m).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();

    for col in 0..m {
        let pivot = (col..m)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap();
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }

        a.swap(col, pivot);
        inv.swap(col, pivot);

        let p = a[col][col];
        for k in 0..m {
            a[col][k] /= p;
            inv[col][k] /= p;
        }

        for row in 0..m {
            if row == col {
                continue;
            }
            let factor = a[row][col];
            if factor == 0.0 {
                continue;
            }
            for k in 0..m {
                a[row][k] -= factor * a[col][k];
                inv[row][k] -= factor * inv[col][k];
            }
        }
    }

    Some(inv)
}
