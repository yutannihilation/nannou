use crate::draw::primitive::path;
use crate::draw::primitive::Primitive;
use crate::draw::properties::spatial::{self, dimension, orientation, position};
use crate::draw::properties::{SetDimensions, SetOrientation, SetPosition};
use crate::draw::{self, Drawing};
use crate::geom::{self, Vector2};
use crate::math::BaseFloat;
use crate::wgpu;

/// Properties related to drawing a **Rect**.
#[derive(Clone, Debug)]
pub struct Texture<'a, S = geom::scalar::Default> {
    texture_view: wgpu::TextureView<'a>,
    spatial: spatial::Properties<S>,
    area: geom::Rect,
}

/// The drawing context for a Rect.
pub type DrawingTexture<'a, S = geom::scalar::Default> = Drawing<'a, Texture<'a, S>, S>;

// Trait implementations.

impl<'a, S> Texture<'a, S>
where
    S: BaseFloat,
{
    pub(crate) fn new(view: &'a dyn wgpu::ToTextureView) -> Self {
        let texture_view = view.to_texture_view();
        let [w, h] = texture_view.size();
        let w = S::from(w).unwrap();
        let h = S::from(h).unwrap();
        let spatial = spatial::Properties::default().w_h(w, h);
        let x = geom::Range {
            start: 0.0,
            end: 1.0,
        };
        let y = geom::Range {
            start: 0.0,
            end: 1.0,
        };
        let area = geom::Rect { x, y };
        Self {
            texture_view,
            spatial,
            area,
        }
    }
}

impl<'a, S> Texture<'a, S> {
    /// Specify the area of the texture to draw.
    ///
    /// The bounds of the rectangle should represent the desired area as texture coordinates of the
    /// underlying texture.
    ///
    /// Texture coordinates range from (0.0, 0.0) in the bottom left of the texture, to (1.0, 1.0)
    /// in the top right of the texture.
    ///
    /// By default, the area represents the full extent of the texture.
    pub fn area(mut self, rect: geom::Rect) -> Self {
        self.area = rect;
        self
    }
}

impl<'a, S> DrawingTexture<'a, S>
where
    S: BaseFloat,
{
    /// Specify the area of the texture to draw.
    ///
    /// The bounds of the rectangle should represent the desired area as texture coordinates of the
    /// underlying texture.
    ///
    /// Texture coordinates range from (0.0, 0.0) in the bottom left of the texture, to (1.0, 1.0)
    /// in the top right of the texture.
    ///
    /// By default, the area represents the full extent of the texture.
    pub fn area(self, rect: geom::Rect) -> Self {
        self.map_ty(|ty| ty.area(rect))
    }
}

impl<'a> draw::renderer::RenderPrimitive<'a> for Texture<'a, f32> {
    fn render_primitive(
        self,
        mut ctxt: draw::renderer::RenderContext<'a>,
        mesh: &mut draw::Mesh,
    ) -> draw::renderer::PrimitiveRender<'a> {
        let Texture {
            texture_view,
            spatial,
            area,
        } = self;
        let spatial::Properties {
            dimensions,
            position,
            orientation,
        } = spatial;

        // If dimensions were specified, scale the points to those dimensions.
        let (maybe_x, maybe_y, maybe_z) = (dimensions.x, dimensions.y, dimensions.z);
        assert!(
            maybe_z.is_none(),
            "z dimension support for rect is unimplemented"
        );
        let w = maybe_x.unwrap_or(100.0);
        let h = maybe_y.unwrap_or(100.0);
        let rect = geom::Rect::from_wh(Vector2 { x: w, y: h });

        // Determine the transform to apply to all points.
        let global_transform = ctxt.transform;
        let local_transform = position.transform() * orientation.transform();
        let transform = global_transform * local_transform;

        // Create an iterator yielding texture points.
        let points_textured = rect
            .corners()
            .vertices()
            .zip(area.invert_y().corners().vertices());

        path::render_path_points_textured(
            points_textured,
            true,
            transform,
            path::Options::Fill(Default::default()),
            &mut ctxt.fill_tessellator,
            &mut ctxt.stroke_tessellator,
            mesh,
        );

        draw::renderer::PrimitiveRender::texture(texture_view)
    }
}

impl<'a, S> SetOrientation<S> for Texture<'a, S> {
    fn properties(&mut self) -> &mut orientation::Properties<S> {
        SetOrientation::properties(&mut self.spatial)
    }
}

impl<'a, S> SetPosition<S> for Texture<'a, S> {
    fn properties(&mut self) -> &mut position::Properties<S> {
        SetPosition::properties(&mut self.spatial)
    }
}

impl<'a, S> SetDimensions<S> for Texture<'a, S> {
    fn properties(&mut self) -> &mut dimension::Properties<S> {
        SetDimensions::properties(&mut self.spatial)
    }
}

// Primitive conversions.

impl<'a, S> From<Texture<'a, S>> for Primitive<'a, S> {
    fn from(prim: Texture<'a, S>) -> Self {
        Primitive::Texture(prim)
    }
}

impl<'a, S> Into<Option<Texture<'a, S>>> for Primitive<'a, S> {
    fn into(self) -> Option<Texture<'a, S>> {
        match self {
            Primitive::Texture(prim) => Some(prim),
            _ => None,
        }
    }
}
