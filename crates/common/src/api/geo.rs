use serde::{Deserialize, Serialize};

/// POST /context request — perception script for a location.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoContextRequest {
    /// Location name or description.
    pub location: String,
}

/// POST /context response — structured text summary.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoContextResponse {
    pub location: String,
    /// Structured text: terrain, borders, nearby features, infrastructure, climate.
    pub context: String,
}

/// POST /spatial/nearby request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoNearbyRequest {
    pub location: String,
    pub radius_km: f64,
    #[serde(default)]
    pub feature_types: Vec<String>,
}

/// POST /spatial/nearby response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoNearbyResponse {
    pub location: String,
    pub radius_km: f64,
    pub features: Vec<GeoFeature>,
}

/// POST /spatial/distance request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDistanceRequest {
    pub from: String,
    pub to: String,
}

/// POST /spatial/distance response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDistanceResponse {
    pub from: String,
    pub to: String,
    pub distance_km: f64,
    pub terrain_description: String,
    pub features_between: Vec<GeoFeature>,
}

/// POST /spatial/route request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoRouteRequest {
    pub origin: String,
    pub destination: String,
}

/// POST /spatial/route response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoRouteResponse {
    pub origin: String,
    pub destination: String,
    pub terrain: String,
    #[serde(default)]
    pub borders_crossed: Vec<String>,
    #[serde(default)]
    pub chokepoints: Vec<String>,
    #[serde(default)]
    pub bodies_of_water: Vec<String>,
    #[serde(default)]
    pub infrastructure: Vec<String>,
}

/// POST /terrain request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoTerrainRequest {
    pub location: String,
}

/// POST /terrain response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoTerrainResponse {
    pub location: String,
    pub elevation: String,
    pub terrain_type: String,
    pub traversability: String,
    pub natural_features: Vec<String>,
}

/// POST /borders request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoBordersRequest {
    pub country: String,
}

/// POST /borders response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoBordersResponse {
    pub country: String,
    pub borders: Vec<GeoBorderInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoBorderInfo {
    pub neighbor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_km: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain_at_border: Option<String>,
    #[serde(default)]
    pub disputed: bool,
}

/// POST /features request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoFeaturesRequest {
    pub region: String,
    #[serde(default)]
    pub feature_types: Vec<String>,
}

/// POST /features response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoFeaturesResponse {
    pub region: String,
    pub features: Vec<GeoFeature>,
}

/// A geographic feature returned by Geo queries.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoFeature {
    pub name: String,
    pub feature_type: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
}

/// GET /capabilities response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoCapabilities {
    pub query_types: Vec<String>,
    pub coverage: Vec<String>,
}
