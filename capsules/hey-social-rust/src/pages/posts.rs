// Posts — multi-photo upload with frosted preview cards.
//
// Mirrors capsules/hey-social/client/src/pages/Posts.jsx in spirit
// (multi-image carousel + caption + per-file progress) but keeps the
// Rust port leaner: no cassette/film-strip SVG decorations yet.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, HtmlInputElement, Url};

use crate::api::posts::{create_post, ipfs_upload_media, CreatePostArgs, MediaTile};
use crate::components::icons::{CameraIcon, ImageIcon};
use crate::components::{FloatingDock, TopHeader};

#[derive(Clone)]
struct StagedFile {
    id: String,
    bytes: Vec<u8>,
    name: String,
    mime: String,
    preview_url: String, // blob: URL, revoked on remove
}

#[component]
pub fn Posts() -> impl IntoView {
    let caption = RwSignal::new(String::new());
    let staged: RwSignal<Vec<StagedFile>> = RwSignal::new(Vec::new());
    let busy = RwSignal::new(false);
    let progress = RwSignal::new(0u32);
    let error = RwSignal::new(String::new());
    let navigate = use_navigate();

    let on_file_change = move |ev: Event| {
        let Some(target) = ev.target() else { return };
        let Ok(input): Result<HtmlInputElement, _> = target.dyn_into() else {
            return;
        };
        let Some(files) = input.files() else { return };
        if files.length() == 0 {
            return;
        }
        error.set(String::new());
        for i in 0..files.length() {
            let Some(file) = files.get(i) else { continue };
            let name = file.name();
            let mime = file.type_();
            let preview = Url::create_object_url_with_blob(&file).unwrap_or_default();
            spawn_local(async move {
                let buf_promise = file.array_buffer();
                let Ok(buf_value) = JsFuture::from(buf_promise).await else {
                    return;
                };
                let array = js_sys::Uint8Array::new(&buf_value);
                let mut bytes = vec![0u8; array.length() as usize];
                array.copy_to(&mut bytes);
                staged.update(|v| {
                    v.push(StagedFile {
                        id: uuid::Uuid::new_v4().to_string(),
                        bytes,
                        name,
                        mime,
                        preview_url: preview,
                    });
                });
            });
        }
        // Reset the input so picking the same file again still fires change.
        input.set_value("");
    };

    let remove_staged = move |id: String| {
        staged.update(|v| {
            if let Some(idx) = v.iter().position(|s| s.id == id) {
                let removed = v.remove(idx);
                let _ = Url::revoke_object_url(&removed.preview_url);
            }
        });
    };

    let submit = move |_| {
        if busy.get() {
            return;
        }
        let files = staged.get();
        if files.is_empty() {
            error.set("Pick at least one photo or video first.".into());
            return;
        }
        let cap = caption.get();
        let navigate = navigate.clone();
        error.set(String::new());
        busy.set(true);
        progress.set(5);
        spawn_local(async move {
            let total = files.len() as u32;
            let mut tiles: Vec<MediaTile> = Vec::with_capacity(files.len());
            for (i, f) in files.iter().enumerate() {
                match ipfs_upload_media(&f.bytes, &f.name, &f.mime).await {
                    Ok(m) => tiles.push(m),
                    Err(e) => {
                        error.set(format!("IPFS upload failed: {e}"));
                        busy.set(false);
                        progress.set(0);
                        return;
                    }
                }
                let pct = 5 + ((i as u32 + 1) * 85 / total.max(1));
                progress.set(pct);
            }
            match create_post(CreatePostArgs {
                caption: cap,
                images: tiles,
            })
            .await
            {
                Ok(_) => {
                    progress.set(100);
                    busy.set(false);
                    // Revoke any preview URLs we created.
                    for f in &files {
                        let _ = Url::revoke_object_url(&f.preview_url);
                    }
                    staged.set(Vec::new());
                    navigate("/", NavigateOptions::default());
                }
                Err(e) => {
                    error.set(format!("Couldn't save post: {e}"));
                    busy.set(false);
                    progress.set(0);
                }
            }
        });
    };

    view! {
        <>
            <TopHeader />
            <FloatingDock />
            <div class="relative mx-auto max-w-3xl space-y-6 pl-24 pr-3 py-6 sm:pl-28 sm:pr-6 sm:py-10">
                <header class="px-1 animate-fade-in">
                    <h1 class="logo-handwritten text-4xl text-primary sm:text-5xl">
                        "Share a moment"
                    </h1>
                    <p class="mt-1 text-sm text-muted">
                        "Photo or short video. Stored on IPFS, pinned to your node, federated to your followers."
                    </p>
                </header>

                <div class="frosted-card p-6 space-y-4 animate-fade-up">
                    <div>
                        <span class="text-[11px] uppercase tracking-wider text-muted">
                            "Media"
                        </span>
                        <div class="mt-2 flex items-center gap-3 flex-wrap">
                            <label class="cursor-pointer inline-flex items-center gap-2 rounded-full bg-white/10 hover:bg-white/20 border border-surface px-4 py-2 text-sm font-medium text-primary">
                                <ImageIcon class="h-4 w-4" />
                                "Choose files"
                                <input
                                    type="file"
                                    class="sr-only"
                                    accept="image/*,video/*"
                                    multiple=true
                                    on:change=on_file_change
                                />
                            </label>
                            {move || {
                                let n = staged.read().len();
                                if n == 0 {
                                    view! { <span class="text-xs text-muted">"No files chosen"</span> }.into_any()
                                } else {
                                    view! { <span class="text-xs text-muted">{format!("{n} file{} ready", if n == 1 { "" } else { "s" })}</span> }.into_any()
                                }
                            }}
                        </div>
                    </div>

                    // Preview row — frosted photo cards, scrollable on mobile,
                    // grid on wider screens.
                    {move || {
                        let files = staged.get();
                        if files.is_empty() {
                            view! { <></> }.into_any()
                        } else {
                            view! {
                                <div class="flex gap-3 overflow-x-auto scroll-snap-x py-1">
                                    <For
                                        each=move || staged.get()
                                        key=|f| f.id.clone()
                                        children=move |f: StagedFile| {
                                            let id_for_remove = f.id.clone();
                                            let click_remove = move |_| remove_staged(id_for_remove.clone());
                                            let is_video = f.mime.starts_with("video/");
                                            view! {
                                                <div class="relative frosted-card overflow-hidden p-0 flex-none w-40 h-40 sm:w-48 sm:h-48 shrink-0 animate-fade-up">
                                                    {if is_video {
                                                        view! {
                                                            <video
                                                                class="block w-full h-full object-cover bg-black"
                                                                src=f.preview_url.clone()
                                                                muted=true
                                                            />
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <img
                                                                class="block w-full h-full object-cover"
                                                                src=f.preview_url.clone()
                                                                alt=f.name.clone()
                                                            />
                                                        }.into_any()
                                                    }}
                                                    <button
                                                        type="button"
                                                        on:click=click_remove
                                                        class="absolute top-1.5 right-1.5 inline-flex h-7 w-7 items-center justify-center rounded-full bg-black/55 text-white hover:bg-black/70 transition-colors"
                                                        aria-label="Remove"
                                                        title="Remove"
                                                    >
                                                        <svg viewBox="0 0 24 24" class="h-3.5 w-3.5" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                                            <path d="M18 6 6 18M6 6l12 12" />
                                                        </svg>
                                                    </button>
                                                    <div class="absolute inset-x-0 bottom-0 px-2 py-1 bg-gradient-to-t from-black/60 to-transparent text-[10px] text-white truncate">
                                                        {f.name.clone()}
                                                    </div>
                                                </div>
                                            }
                                        }
                                    />
                                </div>
                            }.into_any()
                        }
                    }}

                    <div>
                        <span class="text-[11px] uppercase tracking-wider text-muted">
                            "Caption"
                        </span>
                        <textarea
                            class="frosted-input mt-2 text-sm"
                            rows="3"
                            maxlength="2200"
                            placeholder="Say something…"
                            on:input=move |ev: web_sys::Event| {
                                let target = ev.target().unwrap();
                                let ta = target
                                    .dyn_into::<web_sys::HtmlTextAreaElement>()
                                    .unwrap();
                                caption.set(ta.value());
                            }
                        />
                    </div>

                    {move || {
                        let p = progress.get();
                        if p == 0 { view! { <></> }.into_any() }
                        else {
                            view! {
                                <div class="h-1.5 w-full overflow-hidden rounded-full bg-white/10">
                                    <div class="h-full bg-accent transition-[width] duration-300" style=move || format!("width: {}%", progress.get())></div>
                                </div>
                            }.into_any()
                        }
                    }}

                    {move || {
                        let msg = error.get();
                        if msg.is_empty() { view! { <></> }.into_any() }
                        else {
                            view! { <p class="text-sm text-red-400">{msg}</p> }.into_any()
                        }
                    }}

                    <button
                        type="button"
                        on:click=submit
                        prop:disabled=move || busy.get()
                        class="unfrost w-full inline-flex items-center justify-center gap-2 rounded-full bg-accent px-6 py-3 text-sm font-semibold text-accent-text shadow-lg transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-60"
                    >
                        <CameraIcon class="h-4 w-4" />
                        {move || if busy.get() { "Posting…" } else { "Post" }}
                    </button>
                </div>
            </div>
        </>
    }
}
