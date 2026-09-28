"""Pixel-dimension rules shared by every source.

Two things live here because they're really one concern: the minimum size
this catalogue accepts, and how to predict the size a museum's IIIF endpoint
will actually deliver when asked to fit a full-resolution photograph inside a
box without ever enlarging it. Both `nga.py` and `smk.py` request
`.../full/!4000,4000/.../default.jpg`; the `!` means "fit within, don't
upscale," which is exactly what `fit_within` computes locally so the build
doesn't have to download an image just to learn its post-fit size.
"""

# The floor every source applies to `max(width, height)` before a row is
# written. Below this a painting reads as soft or pixellated once it's
# actually stretched across a phone or desktop display.
MIN_LONG_SIDE = 2000

# The size every IIIF request in this catalogue asks for.
IIIF_MAX_SIDE = 4000


def fit_within(width: int, height: int, max_side: int = IIIF_MAX_SIDE) -> tuple[int, int]:
    """The `(width, height)` an IIIF `!max_side,max_side` request actually
    delivers for an image whose real size is `width x height`: unchanged if
    it already fits both axes, otherwise scaled down with the aspect ratio
    preserved. Never scales up — IIIF's `!` size doesn't either.
    """
    if width <= max_side and height <= max_side:
        return width, height
    scale = max_side / max(width, height)
    return max(1, round(width * scale)), max(1, round(height * scale))


def meets_minimum(width: int, height: int) -> bool:
    """The one size gate every row must clear, applied once in `build.py` so
    the four sources can't drift into disagreeing about the number."""
    return max(width, height) >= MIN_LONG_SIDE
