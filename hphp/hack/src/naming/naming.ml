(*
 * Copyright (c) 2015, Facebook, Inc.
 * All rights reserved.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the "hack" directory of this source tree.
 *
 *)

(** Module "naming" a program.
 *
 * The naming phase consists in several things
 * 1- get all the global names
 * 2- transform all the local names into a unique identifier
 *)

open Hh_prelude
open Common
open String_utils
module Env = Naming_phase_env

(*****************************************************************************)
(* Helpers *)
(*****************************************************************************)
let (on_error, reset_errors, get_errors) =
  let naming_errs = ref Naming_phase_error.empty in
  let reset_errors () = naming_errs := Naming_phase_error.empty
  and get_errors () = !naming_errs
  and on_error err = naming_errs := Naming_phase_error.add !naming_errs err in
  (on_error, reset_errors, get_errors)

let elaborate_namespaces =
  new Naming_elaborate_namespaces_endo.generic_elaborator

let invalid_expr_ = Naming_phase_error.invalid_expr_

let mk_env filename tcopt =
  let file_str = Relative_path.suffix filename in
  let dir_str = Filename.dirname file_str in
  let is_hhi = string_ends_with (Relative_path.suffix filename) ".hhi"
  and is_systemlib = TypecheckerOptions.is_systemlib tcopt
  and allow_typeconst_in_enum_class =
    TypecheckerOptions.allow_all_locations_for_type_constant_in_enum_class tcopt
    || List.exists ~f:(fun prefix -> String.is_prefix ~prefix dir_str)
       @@ TypecheckerOptions.allowed_locations_for_type_constant_in_enum_class
            tcopt
  and allow_module_def =
    TypecheckerOptions.allow_all_files_for_module_declarations tcopt
    || List.exists
         ~f:(fun allowed_file ->
           let len = String.length allowed_file in
           if len > 0 then
             match allowed_file.[len - 1] with
             | '*' ->
               let allowed_dir =
                 String.sub allowed_file ~pos:0 ~len:(len - 1)
               in
               String_utils.string_starts_with file_str allowed_dir
             | _ -> String.equal allowed_file file_str
           else
             false)
         (TypecheckerOptions.allowed_files_for_module_declarations tcopt)
  and everything_sdt = TypecheckerOptions.everything_sdt tcopt
  and supportdynamic_type_hint_enabled =
    TypecheckerOptions.experimental_feature_enabled
      tcopt
      TypecheckerOptions.experimental_supportdynamic_type_hint
  and hkt_enabled = TypecheckerOptions.higher_kinded_types tcopt
  and like_type_hints_enabled = TypecheckerOptions.like_type_hints tcopt
  and soft_as_like = TypecheckerOptions.interpret_soft_types_as_like_types tcopt
  and consistent_ctor_level =
    TypecheckerOptions.explicit_consistent_constructors tcopt
  in
  Env.
    {
      empty with
      is_hhi;
      is_systemlib;
      consistent_ctor_level;
      allow_typeconst_in_enum_class;
      allow_module_def;
      everything_sdt;
      supportdynamic_type_hint_enabled;
      hkt_enabled;
      like_type_hints_enabled;
      soft_as_like;
    }

let passes =
  [
    (* Stop on `Invalid` expressions *)
    Naming_guard_invalid.pass;
    (* Canonicalization passes -------------------------------------------- *)
    (* Remove top-level file attributes, noop and markup statements *)
    Naming_elab_defs.pass;
    (* Remove function bodies when in hhi mode *)
    Naming_elab_func_body.pass;
    (* Flatten `Block` statements *)
    Naming_elab_block.pass;
    (* Strip `Hsoft` hints or replace with `Hlike` *)
    Naming_elab_soft.pass;
    (* Elaborate `Happly` to canonical representation, if any *)
    Naming_elab_happly_hint.pass on_error;
    (* Elaborate class identifier expressions (`CIexpr`) to canonical
        representation: `CIparent`, `CIself`, `CIstatic`, `CI` _or_
       `CIexpr (_,_, Lvar _ | This )` *)
    Naming_elab_class_id.pass on_error;
    (* Strip type parameters from type parameters when HKTs are not enabled *)
    Naming_elab_hkt.pass on_error;
    (* Elaborate `Collection` to `ValCollection` or `KeyValCollection` *)
    Naming_elab_collection.pass on_error;
    (* Check that user attributes are well-formed *)
    Naming_elab_user_attributes.pass on_error;
    (* Replace import expressions with invalid expression marker *)
    Naming_elab_import.pass;
    (* Elaborate local variables to canonical representation *)
    Naming_elab_lvar.pass;
    (* Warn of explicit use of builtin enum classes; make subtyping of
       enum classes explicit*)
    Naming_elab_enum_class.pass on_error;
    (* Elaborate class members & xhp attributes  *)
    Naming_elab_class_members.pass on_error;
    (* Elaborate special function calls to canonical representation, if any *)
    Naming_elab_call.top_down_pass;
    Naming_elab_call.bottom_up_pass on_error;
    (* Elaborate invariant calls to canonical representation *)
    Naming_elab_invariant.pass on_error;
    (* -- Mark invalid hints and expressions & miscellaneous validation --- *)
    (* Replace invalid uses of `void` and `noreturn` with `Herr` *)
    Naming_elab_retonly_hint.pass on_error;
    (* Replace invalid uses of wildcard hints with `Herr` *)
    Naming_elab_wildcard_hint.pass on_error;
    (* Replace uses to `self` in shape field names with referenced class *)
    Naming_elab_shape_field_name.top_down_pass;
    Naming_elab_shape_field_name.bottom_up_pass on_error;
    (* Replace invalid uses of `this` hints with `Herr` *)
    Naming_elab_this_hint.pass on_error;
    (* Replace invalid `Haccess` root hints with `Herr` *)
    Naming_elab_haccess_hint.pass on_error;
    (* Replace empty `Tuple`s with invalid expression marker *)
    Naming_elab_tuple.pass on_error;
    (* Validate / replace invalid uses of dynamic classes in `New` and `Class_get`
       expressions *)
    Naming_elab_dynamic_class_name.pass on_error;
    (* Replace non-constant class or global constant with invalid expression marker *)
    Naming_elab_const_expr.top_down_pass;
    Naming_elab_const_expr.bottom_up_pass on_error;
    (* Replace malformed key / value bindings in as expressions with invalid
       local var markers *)
    Naming_elab_as_expr.pass on_error;
    (* Validate hints used in `Cast` expressions *)
    Naming_validate_cast_expr.pass on_error;
    (* Check for duplicate function parameter names *)
    Naming_validate_fun_params.pass on_error;
    (* Validate use of `require implements`, `require extends` and
       `require class` declarations for traits, interfaces and classes *)
    Naming_validate_class_req.pass on_error;
    (* Validation dealing with common xhp naming errors *)
    Naming_validate_xhp_name.pass on_error;
    (* -- Elaboration & validation under typechecker options -------------- *)
    (* Add `supportdyn` and `Like` wrappers everywhere - under `everything-sdt`
       typechecker option *)
    Naming_elab_everything_sdt.top_down_pass;
    Naming_elab_everything_sdt.bottom_up_pass;
    (* Validate use of `Hlike` hints - depends on `enable-like-type-hints`
       and `everything_sdt` typechecker options *)
    Naming_validate_like_hint.pass on_error;
    (* Validate constructors under
       `consistent-explicit_consistent_constructors` typechecker option *)
    Naming_validate_consistent_construct.pass on_error;
    (* Validate  use of `SupportDyn` class - depends on `enable-supportdyn`
       and `everything_sdt` typechecker options *)
    Naming_validate_supportdyn.pass on_error;
    (* Validate uses of enum class type constants - depends on:
       - `allow_all_locations_for_type_constant_in_enum_class`
       - `allowed_locations_for_type_constant_in_enum_class`
       typecheck options
    *)
    Naming_validate_enum_class_typeconst.pass on_error;
    (* Validate use of module definitions - depends on:
        - `allow_all_files_for_module_declarations`
        - `allowed_files_for_module_declarations`
       typechecker options *)
    Naming_validate_module.pass on_error;
  ]

let ( elab_core_program,
      elab_core_class,
      elab_core_fun_def,
      elab_core_module_def,
      elab_core_gconst,
      elab_core_typedef ) =
  Naming_phase_pass.mk_visitor passes

let elab_elem elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core =
  reset_errors ();
  let env = mk_env filename tcopt in
  let elem = elab_ns elem |> elab_capture |> elab_core env in
  Naming_phase_error.emit @@ get_errors ();
  reset_errors ();
  elem

let program_filename defs =
  let open Aast_defs in
  let rec aux = function
    | Fun fun_def :: _ -> Pos.filename fun_def.fd_fun.f_span
    | Class class_ :: _ -> Pos.filename class_.c_span
    | Stmt (pos, _) :: _ -> Pos.filename pos
    | Typedef typedef :: _ -> Pos.filename typedef.t_span
    | Constant gconst :: _ -> Pos.filename gconst.cst_span
    | Module module_def :: _ -> Pos.filename module_def.md_span
    | _ :: rest -> aux rest
    | _ -> Relative_path.default
  in
  aux defs

(**************************************************************************)
(* The entry points to CHECK the program, and transform the program *)
(**************************************************************************)

let program ctx program =
  let filename = program_filename program
  and tcopt = Provider_context.get_tcopt ctx
  and elab_ns =
    elaborate_namespaces#on_program
      (Naming_elaborate_namespaces_endo.make_env
         Namespace_env.empty_with_default)
  and elab_capture = Naming_captures.elab_program
  and elab_core = elab_core_program in
  elab_elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core program

let fun_def ctx fd =
  let filename = Pos.filename fd.Aast.fd_fun.Aast.f_span
  and tcopt = Provider_context.get_tcopt ctx
  and elab_ns =
    elaborate_namespaces#on_fun_def
      (Naming_elaborate_namespaces_endo.make_env fd.Aast.fd_namespace)
  and elab_capture = Naming_captures.elab_fun_def
  and elab_core = elab_core_fun_def in
  elab_elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core fd

let class_ ctx c =
  let filename = Pos.filename c.Aast.c_span
  and tcopt = Provider_context.get_tcopt ctx
  and elab_ns =
    elaborate_namespaces#on_class_
      (Naming_elaborate_namespaces_endo.make_env c.Aast.c_namespace)
  and elab_capture = Naming_captures.elab_class
  and elab_core = elab_core_class in
  elab_elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core c

let module_ ctx md =
  let filename = Pos.filename md.Aast.md_span
  and tcopt = Provider_context.get_tcopt ctx
  and elab_ns =
    elaborate_namespaces#on_module_def
      (Naming_elaborate_namespaces_endo.make_env
         Namespace_env.empty_with_default)
  and elab_capture = Naming_captures.elab_module_def
  and elab_core = elab_core_module_def in
  elab_elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core md

let global_const ctx cst =
  let filename = Pos.filename cst.Aast.cst_span
  and tcopt = Provider_context.get_tcopt ctx
  and elab_ns =
    elaborate_namespaces#on_gconst
      (Naming_elaborate_namespaces_endo.make_env cst.Aast.cst_namespace)
  and elab_capture = Naming_captures.elab_gconst
  and elab_core = elab_core_gconst in
  elab_elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core cst

let typedef ctx td =
  let filename = Pos.filename @@ td.Aast.t_span
  and tcopt = Provider_context.get_tcopt ctx
  and elab_ns =
    elaborate_namespaces#on_typedef
      (Naming_elaborate_namespaces_endo.make_env td.Aast.t_namespace)
  and elab_capture = Naming_captures.elab_typedef
  and elab_core = elab_core_typedef in
  elab_elem ~filename ~tcopt ~elab_ns ~elab_capture ~elab_core td
