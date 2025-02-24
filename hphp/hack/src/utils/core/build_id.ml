(*
 * Copyright (c) 2015, Facebook, Inc.
 * All rights reserved.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the "hack" directory of this source tree.
 *
 *)

open Hh_prelude

external get_build_revision : unit -> string = "hh_get_build_revision"

external get_build_commit_time : unit -> int = "hh_get_build_commit_time"

external get_build_commit_time_string : unit -> string
  = "hh_get_build_commit_time_string"

external get_build_mode : unit -> string = "hh_get_build_mode"

let build_revision = get_build_revision ()

let build_commit_time = get_build_commit_time ()

let build_commit_time_string = get_build_commit_time_string ()

let build_mode = get_build_mode ()

let is_build_optimized =
  String_utils.string_starts_with build_mode "dbgo"
  || String_utils.string_starts_with build_mode "opt"
  || String.equal build_mode ""

let is_dev_build =
  (* FB development build hashes are empty. *)
  String.equal build_revision ""
  (* Dune build hashes are short. *)
  || String.length build_revision <= 16

(* fail open if we don't know build mode *)
