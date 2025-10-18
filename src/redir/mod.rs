use cfg_if::cfg_if;

pub mod bsd_pf;
pub mod redir_ext;
pub mod sys;

cfg_if! {
if #[cfg(any(target_os = "macos",
                target_os = "ios"))] {
        #[path = "pfvar_bindgen_macos.rs"]
        #[allow(dead_code, non_upper_case_globals, non_snake_case, non_camel_case_types)]
        #[allow(clippy::useless_transmute, clippy::too_many_arguments, clippy::unnecessary_cast)]
        mod pfvar;
    }
}
