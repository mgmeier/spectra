use luminance::framebuffer::Framebuffer;
use luminance::pixel::{Depth32F, RGBA32F};
use luminance::tess::{Mode, Tess};
use luminance::texture::{Dim2, Flat, Texture, Unit};
use luminance::pipeline::Pipeline;
use luminance::tess::TessRender;
use std::ops::{Add, Mul, Sub};

pub use luminance::blending::{Equation, Factor};

use color::RGBA;
use resource::{Res, ResCache};
use shader::{Program, Uniform};

/// Simple texture that can be embedded into a compositing graph.
pub type TextureLayer<'a> = &'a ColorMap;

pub type ColorMap = Texture<Flat, Dim2, RGBA32F>;
pub type DepthMap = Texture<Flat, Dim2, Depth32F>;

/// Render layer used to host renders.
pub struct RenderLayer<'a> {
  render: Box<Fn(&Framebuffer<Flat, Dim2, ColorMap, DepthMap>) + 'a>
}

impl<'a> RenderLayer<'a> {
  pub fn new<F>(render: F) -> Self where F: 'a + Fn(&Framebuffer<Flat, Dim2, ColorMap, DepthMap>) {
    RenderLayer {
      render: Box::new(render)
    }
  }
}

/// Compositing node.
pub enum Node<'a> {
  /// A render node.
  ///
  /// Contains render layer.
  Render(RenderLayer<'a>),
  /// A texture node.
  ///
  /// Contains a single texture. The optional `[f32; 2]` is 2D scale applied when sampling the
  /// texture.
  Texture(TextureLayer<'a>, Option<[f32; 2]>),
  /// A single color.
  ///
  /// Keep in mind that such a node is great when you want to display a fullscreen colored quad but
  /// you shouldn’t use it for blending purpose. Adding color masking to your post-process is a
  /// better alternative and will avoid fillrate alteration.
  Color(RGBA),
  /// Composite node.
  ///
  /// Composite nodes are used to blend two compositing nodes according to a given `Equation` and
  /// two blending `Factor`s for source and destination, respectively.
  Composite(Box<Node<'a>>, Box<Node<'a>>, RGBA, Equation, Factor, Factor),
  /// Simple fullscreen effect.
  ///
  /// Such a node is used to apply a user-defined shader on a fullscreen quad. The shader should
  /// provide both the vertex and fragment shader. The vertex shader doesn’t take any inputs but is
  /// invoked in an *attributeless* context on a triangle strip configuration. The fragment shader
  /// should output only one *RGBA* fragment.
  FullscreenEffect(&'a Program)
}

impl<'a> Node<'a> {
  /// Compose this node with another one.
  pub fn compose_with(self, rhs: Self, clear_color: RGBA, eq: Equation, src_fct: Factor, dst_fct: Factor) -> Self {
    Node::Composite(Box::new(self), Box::new(rhs), clear_color, eq, src_fct, dst_fct)
  }

  /// Compose this node over the other. In effect, the resulting node will replace any pixels covered
  /// by the right node by the ones of the left node unless the alpha value is different than `1`.
  /// In that case, an additive blending based on the alpha value of the left node will be performed.
  ///
  /// If you set the alpha value to `0` at a pixel in the left node, then the resulting pixel will be
  /// the one from the right node.
  pub fn over(self, rhs: Self) -> Self {
    rhs.compose_with(self, RGBA::new(0., 0., 0., 0.), Equation::Additive, Factor::SrcAlpha, Factor::SrcAlphaComplement)
  }
}

impl<'a> From<RenderLayer<'a>> for Node<'a> {
  fn from(layer: RenderLayer<'a>) -> Self {
    Node::Render(layer)
  }
}

impl<'a> From<TextureLayer<'a>> for Node<'a> {
  fn from(texture: TextureLayer<'a>) -> Self {
    Node::Texture(texture, Some([1., 1.]))
  }
}

impl<'a> From<RGBA> for Node<'a> {
  fn from(color: RGBA) -> Self {
    Node::Color(color)
  }
}

impl<'a> From<&'a Program> for Node<'a> {
  fn from(program: &'a Program) -> Self {
    Node::FullscreenEffect(program)
  }
}

impl<'a> Add for Node<'a> {
  type Output = Self;

  fn add(self, rhs: Self) -> Self {
    self.compose_with(rhs, RGBA::new(0., 0., 0., 0.), Equation::Additive, Factor::One, Factor::One)
  }
}

impl<'a> Sub for Node<'a> {
  type Output = Self;

  fn sub(self, rhs: Self) -> Self {
    self.compose_with(rhs, RGBA::new(0., 0., 0., 0.), Equation::Subtract, Factor::One, Factor::One)
  }
}

impl<'a> Mul for Node<'a> {
  type Output = Self;

  fn mul(self, rhs: Self) -> Self {
    self.compose_with(rhs, RGBA::new(1., 1., 1., 1.), Equation::Additive, Factor::Zero, Factor::SrcColor)
  }
}

/// Compositor object; used to consume `Node`s and output to screen.
pub struct Compositor {
  // width
  w: u32,
  // height
  h: u32,
  // allocated framebuffers that might contain nodes’ output
  framebuffers: Vec<Framebuffer<Flat, Dim2, ColorMap, DepthMap>>,
  // free list of available framebuffers
  free_framebuffers: Vec<usize>,
  // program used to compose nodes
  compose_program: Res<Program>,
  // program used to render textures scaled
  texture_program: Res<Program>,
  // attributeless fullscreen quad for compositing
  quad: Tess
}

const FORWARD_SOURCE: &'static Uniform<Unit> = &Uniform::new(0);

const TEXTURE_SOURCE: &'static Uniform<Unit> = &Uniform::new(0);
const TEXTURE_SCALE: &'static Uniform<[f32; 2]> = &Uniform::new(1);

impl Compositor {
  pub fn new(w: u32, h: u32, cache: &mut ResCache) -> Self {
    Compositor {
      w: w,
      h: h,
      framebuffers: Vec::new(),
      free_framebuffers: Vec::new(),
      compose_program: cache.get("spectra/compositing/forward.glsl", vec![FORWARD_SOURCE.sem("source")]).unwrap(),
      texture_program: cache.get("spectra/compositing/texture.glsl", vec![
        TEXTURE_SOURCE.sem("source"),
        TEXTURE_SCALE.sem("scale")
      ]).unwrap(),
      quad: Tess::attributeless(Mode::TriangleStrip, 4)
    }
  }

  /// Whenever a node must be composed, we need a framebuffer to render into. This function pulls a
  /// framebuffer to use (via self.framebuffers) by returing an index. It might allocate a new
  /// framebuffer if there isn’t enough framebuffers to be pulled.
  fn pull_framebuffer(&mut self) -> usize {
    self.free_framebuffers.pop().unwrap_or_else(|| {
      let framebuffer_index = self.framebuffers.len();

      let framebuffer = Framebuffer::new((self.w, self.h), 0).unwrap();
      self.framebuffers.push(framebuffer);

      framebuffer_index
    })
  }

  /// Whenever a node has finished being composed, we *might* need to dispose the framebuffer it has
  /// pulled. This funciton does that job.
  ///
  /// It never deallocates memory. It has an important property: once a framebuffer is pulled,
  /// calling that function will make it available for other nodes, improving memory usage for the
  /// next calls.
  #[inline]
  fn dispose_framebuffer(&mut self, framebuffer_index: usize) {
    self.free_framebuffers.push(framebuffer_index);
  }

  /// Consume and display a compositing graph represented by its nodes.
  pub fn display(&mut self, root: Node) {
    let fb_index = self.treat_node(root);

    {
      let fb = &self.framebuffers[fb_index];
      let screen = Framebuffer::default((self.w, self.h));
      let compose_program = self.compose_program.borrow();
      let tess_render = TessRender::from(&self.quad);

      Pipeline::new(&screen, [0., 0., 0., 1.], &[&*fb.color_slot], &[]).enter(|shd_gate| {
        shd_gate.new(&compose_program, &[], &[], &[]).enter(|rdr_gate| {
          rdr_gate.new(None, false, &[], &[], &[]).enter(|tess_gate| {
            let uniforms = [FORWARD_SOURCE.alter(Unit::new(0))];
            tess_gate.render(tess_render, &uniforms, &[], &[])
          });
        });
      });
    }

    self.dispose_framebuffer(fb_index);
  }

  /// Treat a node hierarchy and return the index  of the framebuffer that contains the result.
  fn treat_node(&mut self, node: Node) -> usize {
    match node {
      Node::Render(layer) => self.render(layer),
      Node::Texture(texture, scale) => self.texturize(texture, scale),
      Node::Color(color) => self.colorize(color),
      Node::Composite(left, right, clear_color, eq, src_fct, dst_fct) => self.composite(*left, *right, clear_color, eq, src_fct, dst_fct),
      Node::FullscreenEffect(program) => self.fullscreen_effect(program)
    }
  }

  fn render(&mut self, layer: RenderLayer) -> usize {
    let fb_index = self.pull_framebuffer();
    let fb = &self.framebuffers[fb_index];

    (layer.render)(&fb);

    fb_index
  }

  fn texturize(&mut self, texture: TextureLayer, opt_scale: Option<[f32; 2]>) -> usize {
    let fb_index = self.pull_framebuffer();
    let fb = &self.framebuffers[fb_index];

    let texture_program = self.texture_program.borrow();
    let tess_render = TessRender::from(&self.quad);
    let scale = opt_scale.unwrap_or([1., 1.]);

    Pipeline::new(fb, [0., 0., 0., 1.], &[&**texture], &[]).enter(|shd_gate| {
      shd_gate.new(&texture_program, &[], &[], &[]).enter(|rdr_gate| {
        rdr_gate.new(None, false, &[], &[], &[]).enter(|tess_gate| {
          let uniforms = [
            TEXTURE_SOURCE.alter(Unit::new(0)),
            TEXTURE_SCALE.alter(scale)
          ];
          tess_gate.render(tess_render, &uniforms, &[], &[]);
        });
      });
    });

    fb_index
  }

  fn colorize(&mut self, color: RGBA) -> usize {
    let fb_index = self.pull_framebuffer();
    let fb = &self.framebuffers[fb_index];

    let color = *color.as_ref();

    Pipeline::new(fb, color, &[], &[]).enter(|_| {});

    fb_index
  }

  fn composite(&mut self, left: Node, right: Node, clear_color: RGBA, eq: Equation, src_fct: Factor, dst_fct: Factor) -> usize {
    let left_index = self.treat_node(left);
    let right_index = self.treat_node(right);

    assert!(left_index < self.framebuffers.len());
    assert!(right_index < self.framebuffers.len());

    let fb_index = self.pull_framebuffer();

    {
      let fb = &self.framebuffers[fb_index];

      let left_fb = &self.framebuffers[left_index];
      let right_fb = &self.framebuffers[right_index];

      let texture_set = &[
        &*left_fb.color_slot,
        &*right_fb.color_slot
      ];
      let compose_program = self.compose_program.borrow();
      let tess_render = TessRender::from(&self.quad);

      Pipeline::new(fb, *clear_color.as_ref(), texture_set, &[]).enter(|shd_gate| {
        shd_gate.new(&compose_program, &[], &[], &[]).enter(|rdr_gate| {
          rdr_gate.new((eq, src_fct, dst_fct), false, &[], &[], &[]).enter(|tess_gate| {
            let uniforms = [FORWARD_SOURCE.alter(Unit::new(0))];
            tess_gate.render(tess_render.clone(), &uniforms, &[], &[]);

            let uniforms = [FORWARD_SOURCE.alter(Unit::new(1))];
            tess_gate.render(tess_render, &uniforms, &[], &[]);
          });
        });
      });
    }

    // dispose both left and right framebuffers
    self.dispose_framebuffer(left_index);
    self.dispose_framebuffer(right_index);

    fb_index
  }

  fn fullscreen_effect(&mut self, program: &Program) -> usize {
    let fb_index = self.pull_framebuffer();
    let fb = &self.framebuffers[fb_index];

    let tess_render = TessRender::from(&self.quad);

    Pipeline::new(fb, [0., 0., 0., 1.], &[], &[]).enter(|shd_gate| {
      shd_gate.new(&program, &[], &[], &[]).enter(|rdr_gate| {
        rdr_gate.new(None, false, &[], &[], &[]).enter(|tess_gate| {
          tess_gate.render(tess_render, &[], &[], &[]);
        });
      });
    });

    fb_index
  }
}
