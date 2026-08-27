mod auth;
mod http;
mod library;
mod local_library;
mod local_playlists;
mod playlists;
mod profile;

pub use auth::UserIdentity;
pub use http::{ApiError, BinaryMetadata, BinaryResponse, ByteRange};
pub use library::{
    AlbumPageResponse, AlbumResponse, ArtistPageResponse, ArtistResponse, ArtworkResponse,
    BrowseQuery, GenrePageResponse, GenreResponse, LyricsResponse, PageQuery, PatchAlbumRequest,
    PatchArtistRequest, PatchTrackMetadataRequest, ReplaceLyricsRequest,
    ReplaceTrackMetadataRequest, TrackCredit, TrackMetadataResponse, TrackPageResponse, TrackQuery,
    TrackResponse,
};
pub use local_library::LibraryApi;
pub use local_playlists::PlaylistsApi;
pub use playlists::{
    PlaylistCoverResponse, PlaylistDetailResponse, PlaylistItemResponse, PlaylistItemSort,
    PlaylistResponse, PlaylistSort, PlaylistWriteRequest,
};
pub use profile::{AvatarResponse, ProfileApi};
