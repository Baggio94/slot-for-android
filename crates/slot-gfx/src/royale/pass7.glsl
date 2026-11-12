// crt-royale-fast pass 7: crt-royale-bloom-horizontal-reconstitute.slang
// crt-royale (C) 2014 TroggleMonkey, crt-royale-fast by Hyllian (2024), GPL-2.0-or-later.
// From libretro/slang-shaders crt/shaders/crt-royale/src-fast, converted to GLSL ES 3.00
// by tools/royale/convert.py (parameters fixed at their preset defaults).


uniform vec4 SourceSize;
uniform vec4 OriginalSize;
uniform vec4 OutputSize;
uniform vec4 MASKED_SCANLINESSize;
uniform vec4 BRIGHTPASSSize;

#define MASKED_SCANLINEStexture MASKED_SCANLINES
#define MASKED_SCANLINEStexture_size MASKED_SCANLINESSize.xy
#define MASKED_SCANLINESvideo_size MASKED_SCANLINESSize.xy
#define BRIGHTPASStexture BRIGHTPASS
#define BRIGHTPASStexture_size BRIGHTPASSSize.xy
#define BRIGHTPASSvideo_size BRIGHTPASSSize.xy

#ifndef BIND_SHADER_PARAMS_H
#define BIND_SHADER_PARAMS_H

#ifndef USER_SETTINGS_H
#define USER_SETTINGS_H

#define RUNTIME_SHADER_PARAMS_ENABLE
#define RUNTIME_PHOSPHOR_BLOOM_SIGMA
#define RUNTIME_ANTIALIAS_WEIGHTS

#define RUNTIME_SCANLINES_HORIZ_FILTER_COLORSPACE
#define RUNTIME_GEOMETRY_TILT
#define RUNTIME_GEOMETRY_MODE
#define FORCE_RUNTIME_PHOSPHOR_MASK_MODE_TYPE_SELECT
    #define PHOSPHOR_MASK_MANUALLY_RESIZE
    #define PHOSPHOR_MASK_RESIZE_LANCZOS_WINDOW
    #define PHOSPHOR_BLOOM_TRIADS_LARGER_THAN_3_PIXELS

#define crt_gamma_static (float(2.5))
#define lcd_gamma_static (float(2.2))

#define levels_contrast_static (float(1.0))
#define levels_autodim_temp (float(0.5))

#define halation_weight_static (float(0.0))
#define diffusion_weight_static (float(0.075))
#define bloom_underestimate_levels_static (float(0.8))
#define bloom_excess_static (float(0.0))

#define bloom_approx_filter_static (float(0.0))

#define beam_num_scanlines (float(3.0))
#define beam_generalized_gaussian (bool(true))
#define beam_antialias_level (float(1.0))
#define beam_min_sigma_static (float(0.02))
#define beam_max_sigma_static (float(0.3))
#define beam_spot_shape_function (float(0.0))
#define beam_spot_power_static (float(1.0/3.0))
#define beam_min_shape_static (float(2.0))
#define beam_max_shape_static (float(4.0))
#define beam_shape_power_static (float(1.0/4.0))
#define beam_horiz_filter_static (float(0.0))
#define beam_horiz_sigma_static (float(0.35))
#define beam_horiz_linear_rgb_weight_static (float(1.0))
#define beam_misconvergence (bool(true))
#define convergence_offsets_r_static (vec2(vec2 (0.1, 0.2)))
#define convergence_offsets_g_static (vec2(vec2 (0.3, 0.4)))
#define convergence_offsets_b_static (vec2(vec2 (0.5, 0.6)))
#define interlace_detect_static (bool(true))
#define interlace_1080i_static (bool(false))
#define interlace_bff_static (bool(false))

#define aa_level (float(12.0))
#define aa_filter (float(6.0))
#define aa_temporal (bool(false))
#define aa_subpixel_r_offset_static (vec2(vec2 (-1.0/3.0, 0.0)))
#define aa_cubic_c_static (float(0.5))
#define aa_gauss_sigma_static (float(0.5))

#define mask_type_static (float(1.0))
#define mask_sample_mode_static (float(0.0))
#define mask_specify_num_triads_static (float(0.0))
#define mask_triad_size_desired_static (float(24.0 / 8.0))
#define mask_num_triads_desired_static (float(480.0))
#define mask_sinc_lobes (float(3.0))
#define mask_min_allowed_triad_size (float(2.0))

#define geom_mode_static (float(0.0))
#define geom_radius_static (float(2.0))
#define geom_view_dist_static (float(2.0))
#define geom_tilt_angle_static (vec2(vec2 (0.0, 0.0)))
#define geom_aspect_ratio_static (float(1.313069909))
#define geom_overscan_static (vec2(vec2 (1.0, 1.0)))
#define geom_force_correct_tangent_matrix (bool(true))

#define border_size_static (float(0.015))
#define border_darkness_static (float(2.0))
#define border_compress_static (float(2.5))

#endif

#ifndef DERIVED_SETTINGS_AND_CONSTANTS_H
#define DERIVED_SETTINGS_AND_CONSTANTS_H

#ifndef USER_CGP_CONSTANTS_H
#define USER_CGP_CONSTANTS_H

#define bloom_approx_size_x (float(320.0))
#define bloom_approx_size_x_for_fake (float(400.0))
#define mask_resize_viewport_scale (vec2(vec2(0.0625, 0.0625)))
#define geom_max_aspect_ratio (float(4.0/3.0))

#define mask_texture_small_size (vec2(vec2(64.0, 64.0)))

#define mask_triads_per_tile (float(8.0))

#define mask_grille15_avg_color (float(53.0/255.0))
#define mask_slot_avg_color (float(46.0/255.0))
#define mask_shadow_avg_color (float(41.0/255.0))

#define mask_grille_avg_color (float(mask_grille15_avg_color))

#endif

#define FIX_ZERO(c) (max(abs(c), 0.0000152587890625))

#define bloom_approx_filter (float(bloom_approx_filter_static))
#define mask_resize_src_lut_size (vec2(mask_texture_small_size))

    #ifdef FORCE_RUNTIME_PHOSPHOR_MASK_MODE_TYPE_SELECT
        #define RUNTIME_PHOSPHOR_MASK_MODE_TYPE_SELECT
    #endif

#define max_aa_base_pixel_border (float(0.0))

#define max_aniso_pixel_border (float(max_aa_base_pixel_border + 0.5))

#define max_tiled_pixel_border (float(max_aniso_pixel_border))

#define max_mask_texel_border (float(ceil(max_tiled_pixel_border)))

#define max_mask_tile_border (float(max_mask_texel_border/ (mask_min_allowed_triad_size * mask_triads_per_tile)))

#define mask_resize_num_tiles (float(1.0 + 2.0 * max_mask_tile_border))
#define mask_start_texels (float(max_mask_texel_border))

#define mask_resize_num_triads (float(mask_resize_num_tiles * mask_triads_per_tile))
#define min_allowed_viewport_triads (vec2(vec2(mask_resize_num_triads) / mask_resize_viewport_scale))

#define pi (float(3.141592653589))
#define under_half (float(0.4995))

#endif

#define OVERRIDE_DEVICE_GAMMA
#define gba_gamma (float(3.5))
#define ANTIALIAS_OVERRIDE_BASICS
#define ANTIALIAS_OVERRIDE_PARAMETERS

#define crt_gamma (2.4)

#define lcd_gamma (2.4)

#define levels_contrast (0.671875)

#define bloom_underestimate_levels (1.0)

#define beam_min_sigma (0.055)

#define beam_max_sigma (0.2)

#define beam_spot_power (0.38)

#define beam_min_shape (2.0)

#define beam_max_shape (2.0)

#define beam_shape_power (0.25)

#define beam_horiz_filter (0.0)

#define beam_horiz_sigma (0.35)

#define convergence_offset_x_r (0.0)

#define convergence_offset_x_g (0.0)

#define convergence_offset_x_b (0.0)

#define convergence_offset_y_r (0.05)

#define convergence_offset_y_g (-0.05)

#define convergence_offset_y_b (0.05)

#define mask_type (0.0)

#define aa_subpixel_r_offset_x_runtime (-0.333333333)

#define aa_subpixel_r_offset_y_runtime (0.0)

#define aa_cubic_c (0.5)

#define aa_gauss_sigma (0.5)

#define interlace_detect (bool(bool((1.0))))

#define interlace_1080i (0.0)

vec2 get_aspect_vector(const float geom_aspect_ratio)
{

    float geom_clamped_aspect_ratio =
        min(geom_aspect_ratio, geom_max_aspect_ratio);
    vec2 geom_aspect =
        normalize(vec2(geom_clamped_aspect_ratio, 1.0));
    return geom_aspect;
}

vec3 get_convergence_offsets_x_vector()
{
    return vec3(convergence_offset_x_r, convergence_offset_x_g,
        convergence_offset_x_b);
}

vec3 get_convergence_offsets_y_vector()
{
    return vec3(convergence_offset_y_r, convergence_offset_y_g,
        convergence_offset_y_b);
}

vec2 get_convergence_offsets_r_vector()
{
    return vec2(convergence_offset_x_r, convergence_offset_y_r);
}

vec2 get_convergence_offsets_g_vector()
{
    return vec2(convergence_offset_x_g, convergence_offset_y_g);
}

vec2 get_convergence_offsets_b_vector()
{
    return vec2(convergence_offset_x_b, convergence_offset_y_b);
}

vec2 get_aa_subpixel_r_offset()
{
    return aa_subpixel_r_offset_static;
}

float get_mask_amplify()
{
    float mask_grille_amplify = 1.0/mask_grille_avg_color;
    float mask_slot_amplify = 1.0/mask_slot_avg_color;
    float mask_shadow_amplify = 1.0/mask_shadow_avg_color;
    return mask_type < 0.5 ? mask_grille_amplify :
        mask_type < 1.5 ? mask_slot_amplify :
        mask_shadow_amplify;
}

#endif

#ifndef SCANLINE_FUNCTIONS_H
#define SCANLINE_FUNCTIONS_H

#ifndef SPECIAL_FUNCTIONS_H
#define SPECIAL_FUNCTIONS_H

vec4 erf6(vec4 x)
{

	vec4 one = vec4(1.0);
	vec4 sign_x = sign(x);
	vec4 t = one/(one + 0.47047*abs(x));
	vec4 result = one - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

vec3 erf6(vec3 x)
{

	vec3 one = vec3(1.0);
	vec3 sign_x = sign(x);
	vec3 t = one/(one + 0.47047*abs(x));
	vec3 result = one - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

vec2 erf6(vec2 x)
{

	vec2 one = vec2(1.0);
	vec2 sign_x = sign(x);
	vec2 t = one/(one + 0.47047*abs(x));
	vec2 result = one - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

float erf6(float x)
{

	float sign_x = sign(x);
	float t = 1.0/(1.0 + 0.47047*abs(x));
	float result = 1.0 - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

vec4 erft(vec4 x)
{

	return tanh(1.202760580 * x);
}

vec3 erft(vec3 x)
{

	return tanh(1.202760580 * x);
}

vec2 erft(vec2 x)
{

	return tanh(1.202760580 * x);
}

float erft(float x)
{

	return tanh(1.202760580 * x);
}

vec4 erf(vec4 x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

vec3 erf(vec3 x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

vec2 erf(vec2 x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

float erf(float x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

vec4 gamma_impl(vec4 s, vec4 s_inv)
{

	vec4 g = vec4(1.12906830989);
	vec4 c0 = vec4(0.8109119309638332633713423362694399653724431);
	vec4 c1 = vec4(0.4808354605142681877121661197951496120000040);
	vec4 e = vec4(2.71828182845904523536028747135266249775724709);
	vec4 sph = s + vec4(0.5);
	vec4 lanczos_sum = c0 + c1/(s + vec4(1.0));
	vec4 base = (sph + g)/e;

	return (pow(base, sph) * lanczos_sum) * s_inv;
}

vec3 gamma_impl(vec3 s, vec3 s_inv)
{

	vec3 g = vec3(1.12906830989);
	vec3 c0 = vec3(0.8109119309638332633713423362694399653724431);
	vec3 c1 = vec3(0.4808354605142681877121661197951496120000040);
	vec3 e = vec3(2.71828182845904523536028747135266249775724709);
	vec3 sph = s + vec3(0.5);
	vec3 lanczos_sum = c0 + c1/(s + vec3(1.0));
	vec3 base = (sph + g)/e;
	return (pow(base, sph) * lanczos_sum) * s_inv;
}

vec2 gamma_impl(vec2 s, vec2 s_inv)
{

	vec2 g = vec2(1.12906830989);
	vec2 c0 = vec2(0.8109119309638332633713423362694399653724431);
	vec2 c1 = vec2(0.4808354605142681877121661197951496120000040);
	vec2 e = vec2(2.71828182845904523536028747135266249775724709);
	vec2 sph = s + vec2(0.5);
	vec2 lanczos_sum = c0 + c1/(s + vec2(1.0));
	vec2 base = (sph + g)/e;
	return (pow(base, sph) * lanczos_sum) * s_inv;
}

float gamma_impl(float s, float s_inv)
{

	float g = 1.12906830989;
	float c0 = 0.8109119309638332633713423362694399653724431;
	float c1 = 0.4808354605142681877121661197951496120000040;
	float e = 2.71828182845904523536028747135266249775724709;
	float sph = s + 0.5;
	float lanczos_sum = c0 + c1/(s + 1.0);
	float base = (sph + g)/e;
	return (pow(base, sph) * lanczos_sum) * s_inv;
}

vec4 gamma(vec4 s)
{

	return gamma_impl(s, vec4(1.0)/s);
}

vec3 gamma(vec3 s)
{

	return gamma_impl(s, vec3(1.0)/s);
}

vec2 gamma(vec2 s)
{

	return gamma_impl(s, vec2(1.0)/s);
}

float gamma(float s)
{

	return gamma_impl(s, 1.0/s);
}

vec4 ligamma_small_z_impl(vec4 s, vec4 z, vec4 s_inv)
{

	vec4 scale = pow(z, s);
	vec4 sum = s_inv;

	vec4 z_sq = z*z;
	vec4 denom1 = s + vec4(1.0);
	vec4 denom2 = 2.0*s + vec4(4.0);
	vec4 denom3 = 6.0*s + vec4(18.0);

	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;

	return scale * sum;
}

vec3 ligamma_small_z_impl(vec3 s, vec3 z, vec3 s_inv)
{

	vec3 scale = pow(z, s);
	vec3 sum = s_inv;
	vec3 z_sq = z*z;
	vec3 denom1 = s + vec3(1.0);
	vec3 denom2 = 2.0*s + vec3(4.0);
	vec3 denom3 = 6.0*s + vec3(18.0);
	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;
	return scale * sum;
}

vec2 ligamma_small_z_impl(vec2 s, vec2 z, vec2 s_inv)
{

	vec2 scale = pow(z, s);
	vec2 sum = s_inv;
	vec2 z_sq = z*z;
	vec2 denom1 = s + vec2(1.0);
	vec2 denom2 = 2.0*s + vec2(4.0);
	vec2 denom3 = 6.0*s + vec2(18.0);
	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;
	return scale * sum;
}

float ligamma_small_z_impl(float s, float z, float s_inv)
{

	float scale = pow(z, s);
	float sum = s_inv;
	float z_sq = z*z;
	float denom1 = s + 1.0;
	float denom2 = 2.0*s + 4.0;
	float denom3 = 6.0*s + 18.0;
	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;
	return scale * sum;
}

vec4 uigamma_large_z_impl(vec4 s, vec4 z)
{

	vec4 numerator = pow(z, s) * exp(-z);
	vec4 denom = vec4(7.0) + z - s;
	denom = vec4(5.0) + z - s + (3.0*s - vec4(9.0))/denom;
	denom = vec4(3.0) + z - s + (2.0*s - vec4(4.0))/denom;
	denom = vec4(1.0) + z - s + (s - vec4(1.0))/denom;
	return numerator / denom;
}

vec3 uigamma_large_z_impl(vec3 s, vec3 z)
{

	vec3 numerator = pow(z, s) * exp(-z);
	vec3 denom = vec3(7.0) + z - s;
	denom = vec3(5.0) + z - s + (3.0*s - vec3(9.0))/denom;
	denom = vec3(3.0) + z - s + (2.0*s - vec3(4.0))/denom;
	denom = vec3(1.0) + z - s + (s - vec3(1.0))/denom;
	return numerator / denom;
}

vec2 uigamma_large_z_impl(vec2 s, vec2 z)
{

	vec2 numerator = pow(z, s) * exp(-z);
	vec2 denom = vec2(7.0) + z - s;
	denom = vec2(5.0) + z - s + (3.0*s - vec2(9.0))/denom;
	denom = vec2(3.0) + z - s + (2.0*s - vec2(4.0))/denom;
	denom = vec2(1.0) + z - s + (s - vec2(1.0))/denom;
	return numerator / denom;
}

float uigamma_large_z_impl(float s, float z)
{

	float numerator = pow(z, s) * exp(-z);
	float denom = 7.0 + z - s;
	denom = 5.0 + z - s + (3.0*s - 9.0)/denom;
	denom = 3.0 + z - s + (2.0*s - 4.0)/denom;
	denom = 1.0 + z - s + (s - 1.0)/denom;
	return numerator / denom;
}

vec4 normalized_ligamma_impl(vec4 s, vec4 z,
    vec4 s_inv, vec4 gamma_s_inv)
{

	vec4 thresh = vec4(0.775075);
	bvec4 z_is_large;
	z_is_large.x = z.x > thresh.x;
	z_is_large.y = z.y > thresh.y;
	z_is_large.z = z.z > thresh.z;
	z_is_large.w = z.w > thresh.w;
	vec4 large_z = vec4(1.0) - uigamma_large_z_impl(s, z) * gamma_s_inv;
	vec4 small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;

	bvec4 inverse_z_is_large = not(z_is_large);
	return large_z * vec4(z_is_large) + small_z * vec4(inverse_z_is_large);
}

vec3 normalized_ligamma_impl(vec3 s, vec3 z,
    vec3 s_inv, vec3 gamma_s_inv)
{

	vec3 thresh = vec3(0.775075);
	bvec3 z_is_large;
	z_is_large.x = z.x > thresh.x;
	z_is_large.y = z.y > thresh.y;
	z_is_large.z = z.z > thresh.z;
	vec3 large_z = vec3(1.0) - uigamma_large_z_impl(s, z) * gamma_s_inv;
	vec3 small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;
	bvec3 inverse_z_is_large = not(z_is_large);
	return large_z * vec3(z_is_large) + small_z * vec3(inverse_z_is_large);
}

vec2 normalized_ligamma_impl(vec2 s, vec2 z,
    vec2 s_inv, vec2 gamma_s_inv)
{

	vec2 thresh = vec2(0.775075);
	bvec2 z_is_large;
	z_is_large.x = z.x > thresh.x;
	z_is_large.y = z.y > thresh.y;
	vec2 large_z = vec2(1.0) - uigamma_large_z_impl(s, z) * gamma_s_inv;
	vec2 small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;
	bvec2 inverse_z_is_large = not(z_is_large);
	return large_z * vec2(z_is_large) + small_z * vec2(inverse_z_is_large);
}

float normalized_ligamma_impl(float s, float z,
    float s_inv, float gamma_s_inv)
{

	float thresh = 0.775075;
	bool z_is_large = z > thresh;
	float large_z = 1.0 - uigamma_large_z_impl(s, z) * gamma_s_inv;
	float small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;
	return large_z * float(z_is_large) + small_z * float(!z_is_large);
}

vec4 normalized_ligamma(vec4 s, vec4 z)
{

	vec4 s_inv = vec4(1.0)/s;
	vec4 gamma_s_inv = vec4(1.0)/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

vec3 normalized_ligamma(vec3 s, vec3 z)
{

	vec3 s_inv = vec3(1.0)/s;
	vec3 gamma_s_inv = vec3(1.0)/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

vec2 normalized_ligamma(vec2 s, vec2 z)
{

	vec2 s_inv = vec2(1.0)/s;
	vec2 gamma_s_inv = vec2(1.0)/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

float normalized_ligamma(float s, float z)
{

	float s_inv = 1.0/s;
	float gamma_s_inv = 1.0/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

#endif

vec3 get_gaussian_sigma(vec3 color, float sigma_range)
{
    if(beam_spot_shape_function < 0.5)
    {

        return vec3(beam_min_sigma) + sigma_range * pow(color, vec3(beam_spot_power));
    }
    else
    {

        vec3 color_minus_1 = color - vec3(1.0);

	return vec3(beam_min_sigma) + sigma_range * sqrt(vec3(1.0) - color_minus_1*color_minus_1);
    }
}

vec3 get_generalized_gaussian_beta(vec3 color, float shape_range)
{
    return beam_min_shape + shape_range * pow(color, vec3(beam_shape_power));
}

vec3 scanline_gaussian_integral_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range)
{
    vec3 sigma         = get_gaussian_sigma(color, sigma_range);
    vec3 ph_offset     = vec3(pixel_height * 0.5);
    vec3 denom_inv     = 1.0/(sigma*sqrt(2.0));
    vec3 integral_high = erf((dist + ph_offset)*denom_inv);
    vec3 integral_low  = erf((dist - ph_offset)*denom_inv);

    return color * 0.5*(integral_high - integral_low)/pixel_height;
}

vec3 scanline_generalized_gaussian_integral_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range, float shape_range)
{
    vec3 alpha     = sqrt(2.0) * get_gaussian_sigma(color, sigma_range);
    vec3 beta      = get_generalized_gaussian_beta(color, shape_range);
    vec3 alpha_inv = vec3(1.0)/alpha;
    vec3 s         = vec3(1.0)/beta;
    vec3 ph_offset = vec3(pixel_height * 0.5);

    vec3 gamma_s_inv   = vec3(1.0)/gamma_impl(s, beta);
    vec3 dist1         = dist + ph_offset;
    vec3 dist0         = dist - ph_offset;
    vec3 integral_high = sign(dist1) * normalized_ligamma_impl(s, pow(abs(dist1)*alpha_inv, beta), beta, gamma_s_inv);
    vec3 integral_low  = sign(dist0) * normalized_ligamma_impl(s, pow(abs(dist0)*alpha_inv, beta), beta, gamma_s_inv);

    return color * 0.5*(integral_high - integral_low)/pixel_height;
}

vec3 scanline_gaussian_sampled_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range)
{
    vec3 sigma     = get_gaussian_sigma(color, sigma_range);
    vec3 sigma_inv = vec3(1.0)/sigma;

    vec3 inner_denom_inv = 0.5 * sigma_inv * sigma_inv;
    vec3 outer_denom_inv = sigma_inv/sqrt(2.0*pi);

    if(beam_antialias_level > 0.5)
    {

        vec3 sample_offset = vec3(pixel_height/3.0);
        vec3 dist2         = dist + sample_offset;
        vec3 dist3         = abs(dist - sample_offset);

        vec3 scale   = color/3.0  * outer_denom_inv;
        vec3 weight1 = exp(-( dist* dist)*inner_denom_inv);
        vec3 weight2 = exp(-(dist2*dist2)*inner_denom_inv);
        vec3 weight3 = exp(-(dist3*dist3)*inner_denom_inv);

	return scale * (weight1 + weight2 + weight3);
    }
    else
    {
        return color*exp(-(dist*dist)*inner_denom_inv)*outer_denom_inv;
    }
}

vec3 scanline_generalized_gaussian_sampled_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range, float shape_range)
{
    vec3 alpha = sqrt(2.0) * get_gaussian_sigma(color, sigma_range);
    vec3 beta  = get_generalized_gaussian_beta(color, shape_range);

    vec3 alpha_inv = vec3(1.0)/alpha;
    vec3 beta_inv  = vec3(1.0)/beta;
    vec3 scale     = color * beta * 0.5 * alpha_inv / gamma_impl(beta_inv, beta);

    if(beam_antialias_level > 0.5)
    {

        vec3 sample_offset = vec3(pixel_height/3.0);
        vec3 dist2         = dist + sample_offset;
        vec3 dist3         = abs(dist - sample_offset);

        vec3 weight1 = exp(-pow(abs( dist*alpha_inv), beta));
        vec3 weight2 = exp(-pow(abs(dist2*alpha_inv), beta));
        vec3 weight3 = exp(-pow(abs(dist3*alpha_inv), beta));

	return scale/3.0 * (weight1 + weight2 + weight3);
    }
    else
    {
        return scale * exp(-pow(abs(dist*alpha_inv), beta));
    }
}

vec3 scanline_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range, float shape_range)
{
    if(beam_generalized_gaussian)
    {
        if(beam_antialias_level > 1.5)
        {
            return scanline_generalized_gaussian_integral_contrib(dist, color, pixel_height, sigma_range, shape_range);
        }
        else
        {
            return scanline_generalized_gaussian_sampled_contrib(dist, color, pixel_height, sigma_range, shape_range);
        }
    }
    else
    {
        if(beam_antialias_level > 1.5)
        {
            return scanline_gaussian_integral_contrib(dist, color, pixel_height, sigma_range);
        }
        else
        {
            return scanline_gaussian_sampled_contrib(dist, color, pixel_height, sigma_range);
        }
    }
}

vec3 get_raw_interpolated_color(vec3 color0, vec3 color1, vec3 color2, vec3 color3, vec4 weights)
{

    return max((mat4x3(color0, color1, color2, color3) * weights), 0.0);
}

vec3 get_interpolated_linear_color(vec3 color0, vec3 color1, vec3 color2, vec3 color3, vec4 weights)
{
    float intermediate_gamma = lcd_gamma;

            vec3 linear_mixed_color = get_raw_interpolated_color(color0, color1, color2, color3, weights);

	    vec3 gamma_mixed_color = get_raw_interpolated_color(
                    pow(color0, vec3(1.0/intermediate_gamma)),
                    pow(color1, vec3(1.0/intermediate_gamma)),
                    pow(color2, vec3(1.0/intermediate_gamma)),
                    pow(color3, vec3(1.0/intermediate_gamma)),
                    weights);

            return mix(gamma_mixed_color, linear_mixed_color, (1.0));
}

vec3 get_scanline_color(sampler2D tex, vec2 scanline_uv, vec2 uv_step_x, vec4 weights)
{
    vec3 color1 = texture(tex, scanline_uv).rgb;
    vec3 color2 = texture(tex, scanline_uv + uv_step_x).rgb;
    vec3 color0 = vec3(0.0);
    vec3 color3 = vec3(0.0);

    if(beam_horiz_filter > 0.5)
    {
        color0 = texture(tex, scanline_uv - uv_step_x).rgb;
        color3 = texture(tex, scanline_uv + 2.0 * uv_step_x).rgb;
    }

    return get_interpolated_linear_color(color0, color1, color2, color3, weights);
}

vec3 sample_single_scanline_horizontal(sampler2D tex, vec2 tex_uv, vec2 tex_size, vec2 texture_size_inv)
{
    vec2 curr_texel = tex_uv * tex_size;

    vec2 prev_texel        = floor(curr_texel - vec2(under_half)) + vec2(0.5);
    vec2 prev_texel_hor    = vec2(prev_texel.x, curr_texel.y);
    vec2 prev_texel_hor_uv = prev_texel_hor * texture_size_inv;

    float prev_dist   = curr_texel.x - prev_texel_hor.x;
    vec4 sample_dists = vec4(1.0 + prev_dist, prev_dist, 1.0 - prev_dist, 2.0 - prev_dist);

    vec4 weights;

    if(beam_horiz_filter < 0.5)
    {

        float x  = sample_dists.y;
        float w2 = x*x*x*(x*(x*6.0 - 15.0) + 10.0);
        weights  = vec4(0.0, 1.0 - w2, w2, 0.0);
    }
    else if(beam_horiz_filter < 1.5)
    {

        float inner_denom_inv = 1.0/(2.0*beam_horiz_sigma*beam_horiz_sigma);
        weights               = exp(-(sample_dists*sample_dists)*inner_denom_inv);
    }
    else if(beam_horiz_filter < 2.5)
    {

        vec4 pi_dists = FIX_ZERO(sample_dists * pi);
        weights       = 2.0 * sin(pi_dists) * sin(pi_dists * 0.5)/(pi_dists * pi_dists);
    }
    else
    {

        float x  = sample_dists.y;
        weights  = vec4(0.0, 1.0 - x, x, 0.0);
    }

    vec4 final_weights = weights/dot(weights, vec4(1.0));

    vec2 uv_step_x = vec2(texture_size_inv.x, 0.0);

    return get_scanline_color(tex, prev_texel_hor_uv, uv_step_x, final_weights);
}

vec3 sample_rgb_scanline_horizontal(sampler2D tex, vec2 tex_uv, vec2 tex_size, vec2 texture_size_inv)
{

    if(beam_misconvergence)
    {
        vec3 convergence_offsets_rgb = get_convergence_offsets_x_vector();
        vec3 offset_u_rgb = convergence_offsets_rgb * texture_size_inv.xxx;

	vec2 scanline_uv_r = tex_uv - vec2(offset_u_rgb.r, 0.0);
        vec2 scanline_uv_g = tex_uv - vec2(offset_u_rgb.g, 0.0);
        vec2 scanline_uv_b = tex_uv - vec2(offset_u_rgb.b, 0.0);

	vec3 sample_r = sample_single_scanline_horizontal(tex, scanline_uv_r, tex_size, texture_size_inv);
        vec3 sample_g = sample_single_scanline_horizontal(tex, scanline_uv_g, tex_size, texture_size_inv);
        vec3 sample_b = sample_single_scanline_horizontal(tex, scanline_uv_b, tex_size, texture_size_inv);

	return vec3(sample_r.r, sample_g.g, sample_b.b);
    }
    else
    {
        return sample_single_scanline_horizontal(tex, tex_uv, tex_size, texture_size_inv);
    }
}

vec2 get_last_scanline_uv( vec2 tex_uv,
		           vec2 tex_size, vec2 texture_size_inv,
		           vec2 il_step_multiple,
                           float frame_count, out float dist)
{
    float field_offset = floor(il_step_multiple.y * 0.75)*mod(frame_count + float((0.0)), 2.0);
    vec2  curr_texel   = tex_uv * tex_size;

    vec2  prev_texel_num     = floor(curr_texel - vec2(under_half));
    float wrong_field        = mod(prev_texel_num.y + field_offset, il_step_multiple.y);
    vec2  scanline_texel_num = prev_texel_num - vec2(0.0, wrong_field);

    vec2  scanline_texel = scanline_texel_num + vec2(0.5);
    vec2  scanline_uv    = scanline_texel * texture_size_inv;

    dist = (curr_texel.y - scanline_texel.y)/il_step_multiple.y;

    return scanline_uv;
}

bool is_interlaced(float num_lines)
{

    if(interlace_detect)
    {

        bool sd_interlace = ((num_lines > 288.5) && (num_lines < 576.5));
        bool hd_interlace = bool(interlace_1080i) ? ((num_lines > 1079.5) && (num_lines < 1080.5)) : false;

	return (sd_interlace || hd_interlace);
    }
    else
    {
        return false;
    }
}

#endif

#define GAMMA_OUT(color)    pow(color, vec3(1.0 / lcd_gamma))

#define bloom_approx_scale_x (float(OutputSize.x / SourceSize.y))
#define max_viewport_size_x (float(1080.0*1024.0*(4.0/3.0)))
#define bloom_diff_thresh_ (float(1.0/256.0))
#define mask_min_allowed_tile_size (float(ceil(mask_min_allowed_triad_size * mask_triads_per_tile)))

struct st_gauss{
    vec4 w1_8;
    vec4 w1_8_ratio;
};

#ifdef VERTEX

in vec2 a_pos;
#define Position vec4(a_pos * 2.0 - 1.0, 0.0, 1.0)
#define TexCoord a_pos
out vec2 video_uv;
out vec2 bloom_dxdy;
out float bloom_sigma_runtime;
out vec4 w1_8;
out vec4 w1_8_ratio;
out float weight_sum_inv;
out float undim_mask_contrast_factors;

float get_min_sigma_to_blur_triad(float triad_size,
    float thresh)
{

    return -0.05168 + 0.6113*triad_size -
        1.122*triad_size*sqrt(0.000416 + thresh);

}

float get_fast_gaussian_weight_sum_inv(float sigma)
{

    return min(exp(exp(0.348348412457428/
        (sigma - 0.0860587260734721))), 0.399334576340352/sigma);
}

st_gauss get_blur_fastest_w1_8(float sigma)
{
    float denom_inv = 0.5/(sigma*sigma);
    float w0 = 1.0;
    float w1 = exp(-1.0 * denom_inv);
    float w2 = exp(-4.0 * denom_inv);
    float w3 = exp(-9.0 * denom_inv);
    float w4 = exp(-16.0 * denom_inv);
    float w5 = exp(-25.0 * denom_inv);
    float w6 = exp(-36.0 * denom_inv);
    float w7 = exp(-49.0 * denom_inv);
    float w8 = exp(-64.0 * denom_inv);

    st_gauss blur_weights;

    blur_weights.w1_8.x = w1 + w2;
    blur_weights.w1_8.y = w3 + w4;
    blur_weights.w1_8.z = w5 + w6;
    blur_weights.w1_8.w = w7 + w8;
    blur_weights.w1_8_ratio.x = w2/(blur_weights.w1_8.x);
    blur_weights.w1_8_ratio.y = w4/(blur_weights.w1_8.y);
    blur_weights.w1_8_ratio.z = w6/(blur_weights.w1_8.z);
    blur_weights.w1_8_ratio.w = w8/(blur_weights.w1_8.w);

    return blur_weights;
}

vec2 get_resized_mask_tile_size(vec2 estimated_viewport_size, vec2 estimated_mask_resize_output_size, bool solemnly_swear_same_inputs_for_every_pass)
{

    float tile_aspect_ratio_inv = mask_resize_src_lut_size.y/mask_resize_src_lut_size.x;
    float tile_aspect_ratio     = 1.0/tile_aspect_ratio_inv;
    vec2  tile_aspect           = vec2(1.0, tile_aspect_ratio_inv);

    float desired_tile_size_x = mask_triads_per_tile * mix((3.0), estimated_viewport_size.x / (480.0), (0.0));

    float temp_tile_size_x = min(desired_tile_size_x, mask_resize_src_lut_size.x);

    vec2 temp_tile_size    = temp_tile_size_x * tile_aspect;
    vec2 min_tile_size     = mask_min_allowed_tile_size * tile_aspect;
    vec2 max_tile_size     = estimated_mask_resize_output_size / mask_resize_num_tiles;
    vec2 clamped_tile_size = clamp(temp_tile_size, min_tile_size, max_tile_size);

    float x_tile_size_from_y = clamped_tile_size.y * tile_aspect_ratio;
    float y_tile_size_from_x = mix(clamped_tile_size.y, clamped_tile_size.x * tile_aspect_ratio_inv, float(solemnly_swear_same_inputs_for_every_pass));
    vec2 reclamped_tile_size = vec2(min(clamped_tile_size.x, x_tile_size_from_y), min(clamped_tile_size.y, y_tile_size_from_x));

    vec2 final_resized_tile_size = floor(reclamped_tile_size + vec2(FIX_ZERO(0.0)));

    return final_resized_tile_size;
}

void main()
{
    gl_Position = Position;
    video_uv = TexCoord;

    bloom_dxdy = vec2(1.0/SourceSize.x, 0.0);

    float mask_tile_size_x = get_resized_mask_tile_size(OutputSize.xy, OutputSize.xy * mask_resize_viewport_scale, false).x;

    bloom_sigma_runtime = get_min_sigma_to_blur_triad(mask_tile_size_x / mask_triads_per_tile, bloom_diff_thresh_);

    st_gauss blur_weights = get_blur_fastest_w1_8(bloom_sigma_runtime);

    w1_8       = blur_weights.w1_8;
    w1_8_ratio = blur_weights.w1_8_ratio;

    weight_sum_inv = get_fast_gaussian_weight_sum_inv(bloom_sigma_runtime);

    float undim_factor = 1.0/levels_autodim_temp;

    undim_mask_contrast_factors = undim_factor * get_mask_amplify() * levels_contrast;

}

#else

in vec2 video_uv;
in vec2 bloom_dxdy;
in float bloom_sigma_runtime;
in vec4 w1_8;
in vec4 w1_8_ratio;
in float weight_sum_inv;
in float undim_mask_contrast_factors;
out vec4 FragColor;
uniform sampler2D Source;
uniform sampler2D BRIGHTPASS;
uniform sampler2D MASKED_SCANLINES;
#define bloom_texture Source

vec3 tex2Dblur17fastest(sampler2D tex, vec2 tex_uv, vec2 dxdy, float weight_sum_inv, vec4 w1_8, vec4 w1_8_ratio)
{

    float w0 = 1.0;

    vec3 sum = vec3(0.0,0.0,0.0);

    sum += (w1_8.w * texture(tex, tex_uv - (7.0 + w1_8_ratio.w) * dxdy).rgb);
    sum += (w1_8.z * texture(tex, tex_uv - (5.0 + w1_8_ratio.z) * dxdy).rgb);
    sum += (w1_8.y * texture(tex, tex_uv - (3.0 + w1_8_ratio.y) * dxdy).rgb);
    sum += (w1_8.x * texture(tex, tex_uv - (1.0 + w1_8_ratio.x) * dxdy).rgb);
    sum += (w0 * texture(tex, tex_uv).rgb);
    sum += (w1_8.x * texture(tex, tex_uv + (1.0 + w1_8_ratio.x) * dxdy).rgb);
    sum += (w1_8.y * texture(tex, tex_uv + (3.0 + w1_8_ratio.y) * dxdy).rgb);
    sum += (w1_8.z * texture(tex, tex_uv + (5.0 + w1_8_ratio.z) * dxdy).rgb);
    sum += (w1_8.w * texture(tex, tex_uv + (7.0 + w1_8_ratio.w) * dxdy).rgb);

    return sum * weight_sum_inv;
}

void main()
{
    vec3 blurred_brightpass = tex2Dblur17fastest(bloom_texture, video_uv, bloom_dxdy, weight_sum_inv, w1_8, w1_8_ratio);

    vec3 intensity_dim  = texture(MASKED_SCANLINEStexture, video_uv).rgb;

    vec3 brightpass     = texture(BRIGHTPASStexture, video_uv).rgb;
    vec3 phosphor_bloom = (intensity_dim - brightpass + blurred_brightpass) * undim_mask_contrast_factors;

    FragColor = vec4(GAMMA_OUT(phosphor_bloom), 1.0);
}

#endif
