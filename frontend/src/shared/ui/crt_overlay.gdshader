shader_type canvas_item;
render_mode unshaded;

uniform sampler2D screen_texture : hint_screen_texture, repeat_disable, filter_linear;
uniform vec3 phosphor_tint : source_color = vec3(0.48, 1.0, 0.58);
uniform float phosphor_tint_strength : hint_range(0.0, 1.0) = 0.18;
uniform float curvature : hint_range(0.0, 0.12) = 0.045;
uniform float scanline_strength : hint_range(0.0, 0.5) = 0.18;
uniform float bloom_strength : hint_range(0.0, 1.0) = 0.34;
uniform float ghost_strength : hint_range(0.0, 0.5) = 0.16;
uniform float noise_strength : hint_range(0.0, 0.2) = 0.028;
uniform float flicker_strength : hint_range(0.0, 0.1) = 0.018;

float analog_noise(vec2 point) {
    return fract(sin(dot(point, vec2(12.9898, 78.233)) + TIME * 9.17) * 43758.5453);
}

vec2 curved_uv(vec2 uv) {
    vec2 centered = uv * 2.0 - 1.0;
    float radius_squared = dot(centered, centered);
    centered *= 1.0 + curvature * radius_squared;
    return centered * 0.5 + 0.5;
}

void fragment() {
    vec2 uv = curved_uv(SCREEN_UV);
    float inside = step(0.0, uv.x) * step(uv.x, 1.0)
        * step(0.0, uv.y) * step(uv.y, 1.0);
    uv = clamp(uv, vec2(0.001), vec2(0.999));

    vec2 pixel = SCREEN_PIXEL_SIZE;
    vec3 source = texture(screen_texture, uv).rgb;
    vec3 blur = texture(screen_texture, uv + vec2(pixel.x * 1.5, 0.0)).rgb;
    blur += texture(screen_texture, uv - vec2(pixel.x * 1.5, 0.0)).rgb;
    blur += texture(screen_texture, uv + vec2(0.0, pixel.y * 1.5)).rgb;
    blur += texture(screen_texture, uv - vec2(0.0, pixel.y * 1.5)).rgb;
    blur *= 0.25;

    float ghost = texture(screen_texture, uv - vec2(pixel.x * 2.6, 0.0)).g;
    ghost += texture(screen_texture, uv - vec2(pixel.x * 5.2, 0.0)).g * 0.45;

    float luminance = max(max(source.r, source.g), source.b);
    vec3 monochrome = phosphor_tint * luminance;
    vec3 color = mix(source, monochrome, phosphor_tint_strength);
    vec3 phosphor_bloom = phosphor_tint * max(max(blur.r, blur.g), blur.b);
    color += mix(blur, phosphor_bloom, 0.35) * bloom_strength;
    color += phosphor_tint * ghost * ghost_strength;

    float scanline = 1.0 - scanline_strength
        * (0.5 + 0.5 * sin(uv.y / pixel.y * 3.14159265));
    float grille = 0.97 + 0.03 * sin(uv.x / pixel.x * 2.0943951);
    float flicker = 1.0 + sin(TIME * 42.0) * flicker_strength;
    flicker += sin(TIME * 7.3) * flicker_strength * 0.35;
    float noise = (analog_noise(uv + vec2(TIME * 0.003, 0.0)) - 0.5) * noise_strength;

    vec2 vignette_uv = uv * (1.0 - uv.yx);
    float vignette = pow(clamp(vignette_uv.x * vignette_uv.y * 18.0, 0.0, 1.0), 0.22);
    color = color * scanline * grille * flicker * vignette;
    color += phosphor_tint * noise;

    COLOR = vec4(max(color, vec3(0.0)) * inside, 1.0);
}
