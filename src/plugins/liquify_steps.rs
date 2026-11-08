//! Inline joint-arm intent with the existing array-of-arrays JSON wire format.
use super::Push;
use serde::{
    de::{Error, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, ops::Deref};

const EMPTY: Push = Push {
    x: 0.,
    y: 0.,
    dx: 0.,
    dy: 0.,
    radius: 0.,
    strength: 0.,
};
#[derive(Clone, Copy, Debug)]
pub(crate) struct Step {
    pushes: [Push; 4],
    len: u8,
}
impl Step {
    pub(super) fn copy_from(input: &[Push]) -> Option<Self> {
        if !(1..=4).contains(&input.len()) {
            return None;
        }
        let mut step = Self {
            pushes: [EMPTY; 4],
            len: input.len() as u8,
        };
        step.pushes[..input.len()].copy_from_slice(input);
        Some(step)
    }
}
impl Deref for Step {
    type Target = [Push];
    fn deref(&self) -> &[Push] {
        &self.pushes[..usize::from(self.len)]
    }
}
impl<'a> IntoIterator for &'a Step {
    type Item = &'a Push;
    type IntoIter = std::slice::Iter<'a, Push>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl Serialize for Step {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.deref().serialize(s)
    }
}
impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Arms;
        impl<'de> Visitor<'de> for Arms {
            type Value = Step;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("one through four joint Liquify arms")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Step, A::Error> {
                let mut step = Step {
                    pushes: [EMPTY; 4],
                    len: 0,
                };
                while let Some(push) = seq.next_element::<Push>()? {
                    if step.len == 4 {
                        return Err(A::Error::custom(
                            "A Liquify step supports at most four joint arms",
                        ));
                    }
                    step.pushes[usize::from(step.len)] = push;
                    step.len += 1;
                }
                if step.len == 0 {
                    return Err(A::Error::custom("A Liquify step requires at least one arm"));
                }
                Ok(step)
            }
        }
        d.deserialize_seq(Arms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn inline_arms_retain_legacy_array_wire_and_bounded_size() {
        assert!(std::mem::size_of::<Step>() <= 104);
        let p = Push {
            x: 2.,
            y: 3.,
            dx: 0.5,
            dy: -0.25,
            radius: 4.,
            strength: 0.75,
        };
        for count in 1..=4 {
            let source = vec![p; count];
            let step = Step::copy_from(&source).unwrap();
            assert_eq!(serde_json::to_value(step).unwrap(), json!(source));
            let decoded: Step = serde_json::from_value(json!(source)).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), json!(source));
            assert_eq!(decoded.len(), count);
        }
        assert!(serde_json::from_value::<Step>(json!([])).is_err());
        assert!(serde_json::from_value::<Step>(json!(vec![p; 5])).is_err());
        assert!(serde_json::from_value::<Step>(json!([{"x":1}])).is_err());
    }
}
