# Pairwise result statistics

Note: this is opus written and i don't claim to fully understand everything. I do however understand each part so that's good.

### Notation

- $m$ players, $p$ pairs $(i, j)$, $i < j$, that have data.
- $s \in \mathbb{R}^m$: player strengths.
- $\mu \in \mathbb{R}^p$: mean payoff of $i$ against $j$, one entry per pair. (Stored in the code as an
  antisymmetric $m \times m$ matrix, $\mu_{ji} = -\mu_{ij}$.)
- $A \in \mathbb{R}^{p \times m}$: incidence matrix. The row for pair $(i, j)$ has $+1$ in column $i$ and $-1$
  in column $j$, so $As$ is the vector of predicted differences $s_i - s_j$.
- $W = \operatorname{diag}(w) \in \mathbb{R}^{p \times p}$: inverse-variance weights, $w_{ij} = 1 / \operatorname{Var}(\mu_{ij}) = n_{ij} / s^2_{ij}$ per pair.
- $\mathbf{1} \in \mathbb{R}^m$: the all-ones vector, and $J = \frac{1}{m} \mathbf{1}\mathbf{1}^\top$.

### Rating model

The model is $\mathbb{E}[\mu] = As$, and the ratings minimise the weighted squared error

$$
\min_s \; (\mu - As)^\top W (\mu - As)
$$

Setting the gradient $-2A^\top W(\mu - As)$ to zero gives the normal equations $Ls = b$:

$$
L = A^\top W A \quad (m \times m\text{, the weighted graph Laplacian of the matchup graph}), \qquad b = A^\top W \mu
$$

### Gauge

Every row of $A$ has one $+1$ and one $-1$, so $A\mathbf{1} = 0$ and therefore $L\mathbf{1} = 0$. $L$ is
singular because shifting every strength by the same constant changes nothing. We fix the
gauge with $\mathbf{1}^\top s = 0$ and use the pseudo-inverse. If the matchup graph is connected,
$\operatorname{null}(L) = \operatorname{span}(\mathbf{1})$. $J$ is the projector onto that span ($J\mathbf{1} = \mathbf{1}$, and $Jv = 0$ when
$\mathbf{1}^\top v = 0$), so $L + J$ is invertible and

$$
L^+ = (L + J)^{-1} - J
$$

Also $\mathbf{1}^\top b = (A\mathbf{1})^\top W \mu = 0$, so $Jb = 0$ and

$$
s = L^+ b = (L + J)^{-1} b, \qquad \text{which satisfies } \mathbf{1}^\top s = 0
$$

### Uncertainty

The pair means are independent, and (treating $s^2$ as known) $\operatorname{Cov}(\mu) = W^{-1}$. The
estimate is linear in $\mu$, $s = L^+ A^\top W \mu$, so

$$
\operatorname{Cov}(s) = L^+ A^\top W \, W^{-1} \, W A L^+ = L^+ L L^+ = L^+
$$

Treating $s^2$ as known understates the uncertainty slightly when $n$ is small.

### Goodness of fit

With residuals $r = \mu - As$,

$$
\chi^2 = r^\top W r, \qquad \text{df} = p - \operatorname{rank}(L) = p - (m - 1)
$$

If strength really is additive, $\chi^2$ is roughly chi-squared with $\text{df}$ degrees of freedom,
so $\chi^2 / \text{df} \approx 1$. A value much larger than 1 means the results are not transitive
(e.g. A beats B, B beats C, C beats A), and the single-number ratings hide that.
In that case, check the pairwise means.
