// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the "hack" directory of this source tree.

// NOTE: Most of the types in this file come from runtime/vm/hhbc.h and need to
// be kept in sync.

use bitflags::bitflags;
use once_cell::sync::OnceCell;
use std::collections::{HashMap, HashSet};

#[cfg(fbcode_build)]
mod opcodes;
#[cfg(not(fbcode_build))]
mod opcodes {
    include!(concat!(env!("CMAKE_BINARY_DIR"), "/hphp/tools/opcodes.rs"));
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlavorDesc {
    CUV,
    CV,
    UV,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmType {
    AA,
    BA,
    BLA,
    DA,
    FCA,
    I64A,
    IA,
    ILA,
    ITA,
    IVA,
    KA,
    LA,
    LAR,
    NA,
    NLA,
    OA(&'static str),
    RATA,
    SA,
    SLA,
    VSA,
    //-- These types are only used by HHAS
    /// ARR is an array of ImmType.
    ARR(Box<ImmType>),
    /// BA2 is a [Label; 2] pair.
    BA2,
    /// OAL is an OA with a lifetime attached.
    OAL(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Inputs {
    NOV,
    Fixed(Box<[FlavorDesc]>),
    SMany,
    CMany,
    CUMany,
    MFinal,
    CMFinal(i64),
    FCall { inp: i64, obj: i64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outputs {
    NOV,
    Fixed(Box<[FlavorDesc]>),
    FCall,
}

bitflags! {
    pub struct InstrFlags: u32 {
        const NF = 0b00000001;
        const TF = 0b00000010;
        const CF = 0b00000100;
        /// This flag indicates that the opcode should be generated as a
        /// structured variant instead of a tuple.
        const AS_STRUCT = 0b00001000;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpcodeData {
    pub name: &'static str,
    pub immediates: Vec<(&'static str, ImmType)>,
    pub inputs: Inputs,
    pub outputs: Outputs,
    pub flags: InstrFlags,
}

mod fixups {
    use super::*;
    use maplit::hashmap;

    pub(crate) trait Action {
        fn perform(&self, opcode: &mut OpcodeData);

        fn imm_position(&self, opcode: &OpcodeData, imm_name: &str) -> usize {
            if let Some(n) = opcode
                .immediates
                .iter()
                .position(|(name, _)| *name == imm_name)
            {
                n
            } else {
                panic!(
                    "For opcode {:?} field {:?} not found.",
                    opcode.name, imm_name
                );
            }
        }
    }

    struct AddFlag {
        flags: InstrFlags,
    }

    impl Action for AddFlag {
        fn perform(&self, opcode: &mut OpcodeData) {
            opcode.flags |= self.flags
        }
    }

    #[allow(dead_code)]
    pub(crate) fn add_flag(flags: InstrFlags) -> Box<dyn Action> {
        Box::new(AddFlag { flags })
    }

    struct RemoveImm {
        imm_name: &'static str,
    }

    impl Action for RemoveImm {
        fn perform(&self, opcode: &mut OpcodeData) {
            let idx = self.imm_position(opcode, self.imm_name);
            opcode.immediates.remove(idx);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn remove_imm(imm_name: &'static str) -> Box<dyn Action> {
        Box::new(RemoveImm { imm_name })
    }

    struct RenameImm {
        old_name: &'static str,
        new_name: &'static str,
    }

    impl Action for RenameImm {
        fn perform(&self, opcode: &mut OpcodeData) {
            let idx = self.imm_position(opcode, self.old_name);
            opcode.immediates[idx].0 = self.new_name;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn rename_imm(old_name: &'static str, new_name: &'static str) -> Box<dyn Action> {
        Box::new(RenameImm { old_name, new_name })
    }

    struct ReplaceImm {
        imm_name: &'static str,
        expected: ImmType,
        repl: ImmType,
    }

    impl Action for ReplaceImm {
        fn perform(&self, opcode: &mut OpcodeData) {
            let idx = self.imm_position(opcode, self.imm_name);
            let imm_ty = &mut opcode.immediates[idx].1;
            if *imm_ty != self.expected {
                panic!(
                    "For opcode {:?} field {:?} expected {:?} but got {:?}",
                    opcode.name, self.imm_name, self.expected, imm_ty
                );
            }
            *imm_ty = self.repl.clone();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn replace_imm(
        imm_name: &'static str,
        expected: ImmType,
        repl: ImmType,
    ) -> Box<dyn Action> {
        Box::new(ReplaceImm {
            imm_name,
            expected,
            repl,
        })
    }

    struct InsertImm {
        idx: usize,
        imm_name: &'static str,
        ty: ImmType,
    }

    impl Action for InsertImm {
        fn perform(&self, opcode: &mut OpcodeData) {
            opcode
                .immediates
                .insert(self.idx, (self.imm_name, self.ty.clone()));
        }
    }

    #[allow(dead_code)]
    pub(crate) fn insert_imm(idx: usize, imm_name: &'static str, ty: ImmType) -> Box<dyn Action> {
        Box::new(InsertImm { idx, imm_name, ty })
    }

    type FixupTable = HashMap<&'static str, Vec<Box<dyn Action>>>;

    // These fixups define extra information to turn simple types (like IVA) to
    // a more complex type (like StackIndex).
    fn get_fixups() -> FixupTable {
        hashmap! {
            "AssertRATStk" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "BaseC" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "BaseGC" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex"))
            ],
            "BaseSC" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
                replace_imm("arg2", ImmType::IVA, ImmType::OA("StackIndex"))
            ],
            "CheckProp" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("PropId")),
            ],
            "ClsCns" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ConstId"))
            ],
            "ClsCnsD" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ConstId")),
                replace_imm("str2", ImmType::SA, ImmType::OAL("ClassId"))
            ],
            "CnsE" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ConstId")),
            ],
            "CreateCl" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("NumParams")),
                replace_imm("arg2", ImmType::IVA, ImmType::OA("ClassNum")),
            ],
            "FCallClsMethodD" => vec![
                replace_imm("str3", ImmType::SA, ImmType::OAL("ClassId")),
                replace_imm("str4", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "FCallClsMethodSD" => vec![
                replace_imm("str4", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "FCallFuncD" => vec![
                replace_imm("str2", ImmType::SA, ImmType::OAL("FunctionId")),
            ],
            "FCallObjMethodD" => vec![
                replace_imm("str4", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "IncDecM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "InitProp" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("PropId")),
            ],
            "InstanceOfD" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
            ],
            "LazyClass" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
            ],
            "MemoGetEager" => vec![
                replace_imm("target1", ImmType::BA, ImmType::BA2),
                remove_imm("target2"),
            ],
            "NewObjD" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
            ],
            "NewObjRD" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
            ],
            "QueryM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "ResolveClass" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
            ],
            "ResolveClsMethod" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "ResolveClsMethodD" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
                replace_imm("str2", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "ResolveClsMethodS" => vec![
                replace_imm("str2", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "ResolveFunc" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("FunctionId")),
            ],
            "ResolveMethCaller" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("FunctionId")),
            ],
            "ResolveRClsMethod" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "ResolveRClsMethodD" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("ClassId")),
                replace_imm("str2", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "ResolveRClsMethodS" => vec![
                replace_imm("str2", ImmType::SA, ImmType::OAL("MethodId")),
            ],
            "ResolveRFunc" => vec![
                replace_imm("str1", ImmType::SA, ImmType::OAL("FunctionId")),
            ],
            "RetM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "SetM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "SetOpM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "SetRangeM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "SSwitch" => vec![
                // Instead of using a single [(String, Label)] field in HHAS we
                // split the cases and targets.
                add_flag(InstrFlags::AS_STRUCT),
                insert_imm(0, "cases", ImmType::ARR(Box::new(ImmType::SA))),
                replace_imm("targets", ImmType::SLA, ImmType::BLA),
            ],
            "UnsetM" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OA("StackIndex")),
            ],
            "VerifyOutType" => vec![
                replace_imm("arg1", ImmType::IVA, ImmType::OAL("ParamId")),
            ],
            "VerifyParamType" => vec![
                replace_imm("loc1", ImmType::ILA, ImmType::OAL("ParamId")),
            ],
            "VerifyParamTypeTS" => vec![
                replace_imm("loc1", ImmType::ILA, ImmType::OAL("ParamId")),
            ],
        }
    }

    pub(crate) fn apply_fixups(opcode: &mut OpcodeData, fixups: &FixupTable) -> bool {
        if let Some(fixes) = fixups.get(opcode.name) {
            for fix in fixes {
                fix.perform(opcode);
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn clone_with_fixups(source: &[OpcodeData]) -> Box<[OpcodeData]> {
        let fixups = fixups::get_fixups();

        let mut output: Vec<OpcodeData> = Vec::new();
        let mut pending_fixups: HashSet<&'static str> = fixups.keys().copied().collect();
        for opcode in source {
            let mut opcode = opcode.clone();
            if apply_fixups(&mut opcode, &fixups) {
                pending_fixups.remove(opcode.name);
            }
            output.push(opcode);
        }

        if let Some(missing) = pending_fixups.into_iter().next() {
            panic!("Fixup requested for missing opcode {:?}", missing);
        }

        output.into()
    }
}

/// This function is used to apply tweaks to the source opcode data.
pub fn opcode_data() -> &'static [OpcodeData] {
    static INSTANCE: OnceCell<Box<[OpcodeData]>> = OnceCell::new();
    INSTANCE.get_or_init(|| fixups::clone_with_fixups(opcodes::opcode_data()))
}

#[cfg(test)]
mod test {
    use super::*;
    use fixups::{add_flag, insert_imm, remove_imm, rename_imm, replace_imm};
    use maplit::hashmap;

    #[test]
    fn test_replace_imm() {
        let mut opcode = OpcodeData {
            name: "TestOp",
            immediates: vec![("field1", ImmType::BLA)],
            inputs: Inputs::NOV,
            outputs: Outputs::NOV,
            flags: InstrFlags::NF,
        };

        let fixups = hashmap! {
            "TestOp" => vec! {
                replace_imm("field1", ImmType::BLA, ImmType::OA("Thing")),
            },
        };

        fixups::apply_fixups(&mut opcode, &fixups);
        assert_eq!(
            opcode,
            OpcodeData {
                name: "TestOp",
                immediates: vec![("field1", ImmType::OA("Thing"))],
                inputs: Inputs::NOV,
                outputs: Outputs::NOV,
                flags: InstrFlags::NF,
            }
        );
    }

    #[test]
    fn test_remove_imm() {
        let mut opcode = OpcodeData {
            name: "TestOp",
            immediates: vec![("field1", ImmType::BLA), ("field2", ImmType::BLA)],
            inputs: Inputs::NOV,
            outputs: Outputs::NOV,
            flags: InstrFlags::NF,
        };

        let fixups = hashmap! {
            "TestOp" => vec! {
                remove_imm("field1"),
            },
        };

        fixups::apply_fixups(&mut opcode, &fixups);
        assert_eq!(
            opcode,
            OpcodeData {
                name: "TestOp",
                immediates: vec![("field2", ImmType::BLA)],
                inputs: Inputs::NOV,
                outputs: Outputs::NOV,
                flags: InstrFlags::NF,
            }
        );
    }

    #[test]
    fn test_add_flag() {
        let mut opcode = OpcodeData {
            name: "TestOp",
            immediates: vec![],
            inputs: Inputs::NOV,
            outputs: Outputs::NOV,
            flags: InstrFlags::NF,
        };

        let fixups = hashmap! {
            "TestOp" => vec! {
                add_flag(InstrFlags::AS_STRUCT),
            },
        };

        fixups::apply_fixups(&mut opcode, &fixups);
        assert_eq!(
            opcode,
            OpcodeData {
                name: "TestOp",
                immediates: vec![],
                inputs: Inputs::NOV,
                outputs: Outputs::NOV,
                flags: InstrFlags::NF | InstrFlags::AS_STRUCT,
            }
        );
    }

    #[test]
    fn test_insert_imm() {
        let mut opcode = OpcodeData {
            name: "TestOp",
            immediates: vec![],
            inputs: Inputs::NOV,
            outputs: Outputs::NOV,
            flags: InstrFlags::NF,
        };

        let fixups = hashmap! {
            "TestOp" => vec! {
                insert_imm(0, "imm1", ImmType::SA),
            },
        };

        fixups::apply_fixups(&mut opcode, &fixups);
        assert_eq!(
            opcode,
            OpcodeData {
                name: "TestOp",
                immediates: vec![("imm1", ImmType::SA)],
                inputs: Inputs::NOV,
                outputs: Outputs::NOV,
                flags: InstrFlags::NF,
            }
        );
    }

    #[test]
    fn test_rename_imm() {
        let mut opcode = OpcodeData {
            name: "TestOp",
            immediates: vec![("field1", ImmType::BLA), ("field2", ImmType::BLA)],
            inputs: Inputs::NOV,
            outputs: Outputs::NOV,
            flags: InstrFlags::NF,
        };

        let fixups = hashmap! {
            "TestOp" => vec! {
                rename_imm("field1", "renamed"),
            },
        };

        fixups::apply_fixups(&mut opcode, &fixups);
        assert_eq!(
            opcode,
            OpcodeData {
                name: "TestOp",
                immediates: vec![("renamed", ImmType::BLA), ("field2", ImmType::BLA)],
                inputs: Inputs::NOV,
                outputs: Outputs::NOV,
                flags: InstrFlags::NF,
            }
        );
    }
}