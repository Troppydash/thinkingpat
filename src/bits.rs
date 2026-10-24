pub fn pext(x: u64, mut mask: u64) -> u64 {
    #[cfg(target_feature = "bmi2")]
    {
        unsafe { core::arch::x86_64::_pext_u64(x, mask) }
    }

    #[cfg(not(target_feature = "bmi2"))]
    {
        let mut out = 0;
        let mut i = 0;
        while mask > 0 {
            let bit = mask.isolate_lowest_one();
            if (x & bit) > 0 {
                out |= 1 << i;
            }
            mask &= !bit;
            i += 1;

        }

        out
    }
}

pub fn pdep(x: u64, mask: u64) -> u64 {
    #[cfg(target_feature = "bmi2")]
    {
        unsafe { core::arch::x86_64::_pdep_u64(x, mask) }
    }

    #[cfg(not(target_feature = "bmi2"))]
    {
        let mut source = x;
        let mut available = mask;
        let mut out = 0;
        while source != 0 {
            let lowest = available.isolate_lowest_one();
            if source & 1 != 0 {
                out |= lowest;
            }
            source >>= 1;
            available ^= lowest;
        }
        out
    }
}
