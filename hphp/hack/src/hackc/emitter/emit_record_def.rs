// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the "hack" directory of this source tree.

use env::emitter::Emitter;
use ffi::{Maybe, Maybe::*, Slice, Str};
use hhas_pos::HhasSpan;
use hhas_record_def::{Field as RecordField, HhasRecord};
use hhas_type::constraint;
use hhbc_id::record::RecordType;
use hhbc_string_utils as string_utils;
use instruction_sequence::Result;
use oxidized::ast::*;

fn valid_tc_for_record_field(tc: &constraint::Constraint<'_>) -> bool {
    match &tc.name {
        Nothing => true,
        Just(name) => {
            !(name.unsafe_as_str().eq_ignore_ascii_case("hh\\this")
                || name.unsafe_as_str().eq_ignore_ascii_case("callable")
                || name.unsafe_as_str().eq_ignore_ascii_case("hh\\nothing")
                || name.unsafe_as_str().eq_ignore_ascii_case("hh\\noreturn"))
        }
    }
}

fn emit_field<'a, 'arena, 'decl>(
    emitter: &Emitter<'arena, 'decl>,
    field: &'a (Sid, Hint, Option<Expr>),
) -> Result<RecordField<'arena>> {
    let (Id(pos, name), hint, expr_opt) = field;
    let otv = expr_opt
        .as_ref()
        .and_then(|e| ast_constant_folder::expr_to_typed_value(emitter, e).ok());
    let ti = emit_type_hint::hint_to_type_info(
        emitter.alloc,
        &emit_type_hint::Kind::Property,
        false,
        false,
        &[],
        &hint,
    )?;
    if valid_tc_for_record_field(&ti.type_constraint) {
        Ok(RecordField(
            Str::new_str(emitter.alloc, name.as_str()),
            ti,
            Maybe::from(otv),
        ))
    } else {
        let name = string_utils::strip_global_ns(name);
        Err(emit_fatal::raise_fatal_parse(
            &pos,
            format!("Invalid record field type hint for '{}'", name),
        ))
    }
}

fn emit_record_def<'a, 'arena, 'decl>(
    emitter: &Emitter<'arena, 'decl>,
    rd: &'a RecordDef,
) -> Result<HhasRecord<'arena>> {
    fn elaborate<'arena>(alloc: &'arena bumpalo::Bump, Id(_, name): &Id) -> RecordType<'arena> {
        RecordType(Str::new_str(alloc, name.trim_start_matches('\\')))
    }
    let parent_name = match &rd.extends {
        Some(Hint(_, h)) => {
            if let Hint_::Happly(id, _) = h.as_ref() {
                Some(elaborate(emitter.alloc, id))
            } else {
                None
            }
        }
        _ => None,
    };
    let fields = rd
        .fields
        .iter()
        .map(|f| emit_field(emitter, &f))
        .collect::<Result<Vec<_>>>()?;
    Ok(HhasRecord {
        name: elaborate(emitter.alloc, &rd.name),
        is_abstract: rd.abstract_,
        base: Maybe::from(parent_name),
        fields: Slice::fill_iter(emitter.alloc, fields.into_iter()),
        span: HhasSpan::from_pos(&rd.span),
    })
}

pub fn emit_record_defs_from_program<'a, 'arena, 'decl>(
    emitter: &Emitter<'arena, 'decl>,
    p: &'a [Def],
) -> Result<Vec<HhasRecord<'arena>>> {
    p.iter()
        .filter_map(|d| d.as_record_def().map(|r| emit_record_def(emitter, r)))
        .collect::<Result<Vec<_>>>()
}