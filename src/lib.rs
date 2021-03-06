//! Demoscene framework.
//!
//! # Foreword
//!
//! This framework is intented to be used to build up [demos](https://en.wikipedia.org/wiki/Demoscene)
//! as a primary purpose. Everything was designed to quickly write demoscene effects and edit them
//! into an audiovisual executable. However, because a lot was added – especially for debugging
//! purposes, it *should* be possible to use this framework for other purposes as well – among
//! *simulations*, *animations* and *video games*.
//!
//! # Design
//!
//! This framework was designed to be *simple* and *flexible*. Following that path, it’s not
//! impossible that some of its features get moved out of the framework to become a set of
//! dependencies – so that people who don’t want those features can just preclude them from the
//! compilation chain.
//! 
//! Up to now, the framework provides you with several modules:
//!
//! - **audio**: this module gives you the ability to play a soundtrack (no streaming implemented
//!   yet though; the whole soundtrack is loaded into memory) and interact with basic yet useful
//!   information about playback (play, pause, toggle, track length, track cursor, etc.)
//! - **bootstrapping**: this module abstracts over the underlying technologies and provides several
//!   simple types that can be used to interact with the demo, such as initialization, default
//!   event handling, and so on
//! - **camera**: provides some camera features for both release and debugging purposes
//! - **color**: color types
//! - **edit**: everything you need to edit your demo – it provides types and functions to reason
//!   about timelines, tracks, cuts and clips, hence easing the overall making of a demo
//! - **gui**: this module provides some GUI code that you can use to build nice debugging
//!   interfaces – up to now, it’s not designed for release code, but it might be at some time
//! - **linear**: linear algebra
//! - **model**: this module provides all the code required to abstract other meshes and add them
//!   the concept of *materials*
//! - **object**: linked to **models**, this module adds the concept of *space properties* to
//!   *models* – so that you can actually have them in your scenes
//! - **overlay**: this module provides 2D primitives and rendering functions
//! - **projection**: projection trait and functions
//! - **shader**: provides 
//! - **extra**: some extra (but not mandatory) other modules

#![feature(associated_consts)]
#![feature(conservative_impl_trait)]
#![feature(const_fn)]

extern crate alto;
extern crate any_cache;
extern crate chrono;
extern crate gl;
extern crate glfw;
extern crate image;
pub extern crate luminance;
extern crate nalgebra;
extern crate notify;
extern crate num;
extern crate rusttype;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate vorbis;
extern crate wavefront_obj;

#[macro_use]
pub mod report;

pub mod audio;
pub mod bootstrap;
pub mod camera;
pub mod compositing;
pub mod color;
pub mod edit;
pub mod extra;
pub mod framebuffer;
pub mod gui;
pub mod light;
pub mod linear;
pub mod model;
pub mod object;
pub mod overlay;
pub mod projection;
pub mod resource;
pub mod shader;
pub mod spline;
pub mod text;
pub mod texture;
pub mod transform;
