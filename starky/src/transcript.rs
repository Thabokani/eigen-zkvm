use crate::errors::Result;
use crate::poseidon_opt::Poseidon;
use crate::traits::FieldExtension;
use crate::traits::Transcript;
use num_bigint::BigUint;
use plonky::field_gl::Fr as FGL;

pub struct TranscriptGL {
    state: [FGL; 4],
    poseidon: Poseidon,
    pending: Vec<FGL>,
    out: Vec<FGL>,
}

impl TranscriptGL {
    fn update_state(&mut self) -> Result<()> {
        while self.pending.len() < 8 {
            self.pending.push(FGL::ZERO);
        }
        self.out = self.poseidon.hash(&self.pending, &self.state, 12)?;

        self.pending = vec![];
        self.state.copy_from_slice(&self.out[0..4]);
        Ok(())
    }
    fn add_1(&mut self, e: &FGL) -> Result<()> {
        log::trace!("add_1: {}", e);
        self.out = Vec::new();
        self.pending.push(*e);
        if self.pending.len() == 8 {
            self.update_state()?;
        }
        Ok(())
    }
}

impl Transcript for TranscriptGL {
    // TODO:Check the type F is equal to F3G after we support F5G.
    fn new() -> Self {
        Self {
            state: [FGL::ZERO; 4],
            poseidon: Poseidon::new(),
            pending: Vec::new(),
            out: Vec::new(),
        }
    }

    fn get_field<F: FieldExtension>(&mut self) -> F {
        let a = self.get_fields1().unwrap();
        let b = self.get_fields1().unwrap();
        let c = self.get_fields1().unwrap();
        F::from_vec(vec![a, b, c])
    }

    fn get_fields1(&mut self) -> Result<FGL> {
        if !self.out.is_empty() {
            let v = self.out[0];
            self.out.remove(0);
            return Ok(v);
        }
        self.update_state()?;
        self.get_fields1()
    }

    fn put(&mut self, es: &[Vec<FGL>]) -> Result<()> {
        for e in es.iter() {
            for t in e {
                self.add_1(t)?;
            }
        }
        Ok(())
    }

    fn get_permutations(&mut self, n: usize, nbits: usize) -> Result<Vec<usize>> {
        let total_bits = n * nbits;
        let n_fields = (total_bits - 1) / 63 + 1;
        let mut fields: Vec<BigUint> = Vec::new();
        for _i in 0..n_fields {
            let e = self.get_fields1()?;
            fields.push(BigUint::from(e.as_int()));
        }
        let mut res: Vec<usize> = vec![];
        let mut cur_field = 0;
        let mut cur_bit = 0usize;
        let one = BigUint::from(1u32);
        for _i in 0..n {
            let mut a = 0usize;
            for j in 0..nbits {
                let shift = &fields[cur_field] >> cur_bit;
                let bit = shift & &one;
                if bit == one {
                    a += 1 << j;
                }
                cur_bit += 1;
                if cur_bit == 63 {
                    cur_bit = 0;
                    cur_field += 1;
                }
            }
            res.push(a);
        }
        Ok(res)
    }
}
