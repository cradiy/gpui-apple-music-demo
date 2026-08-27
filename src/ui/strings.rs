pub fn not_playing() -> &'static str {
    "Not Playing"
}

pub fn choose_a_track() -> &'static str {
    "Choose a track from your library"
}

pub fn untitled() -> &'static str {
    "Untitled"
}

pub fn unknown_artist() -> &'static str {
    "Unknown Artist"
}

pub fn unknown_track() -> &'static str {
    "Unknown Track"
}

pub fn search_music() -> &'static str {
    "Search music"
}

pub fn find_in_songs() -> &'static str {
    "Find in Songs"
}

pub fn find_in_playlist() -> &'static str {
    "Find in Playlist"
}

pub fn songs() -> &'static str {
    "Songs"
}

pub fn home() -> &'static str {
    "Home"
}

pub fn recently_added() -> &'static str {
    "Recently Added"
}

pub fn artists() -> &'static str {
    "Artists"
}

pub fn albums() -> &'static str {
    "Albums"
}

pub fn artist() -> &'static str {
    "Artist"
}

pub fn album() -> &'static str {
    "Album"
}

pub fn loading_artists() -> &'static str {
    "Loading artists…"
}

pub fn loading_albums() -> &'static str {
    "Loading albums…"
}

pub fn loading_artist() -> &'static str {
    "Loading artist…"
}

pub fn loading_album() -> &'static str {
    "Loading album…"
}

pub fn artists_empty() -> &'static str {
    "No artists in your library yet"
}

pub fn albums_empty() -> &'static str {
    "No albums in your library yet"
}

pub fn all_playlists() -> &'static str {
    "All Playlists"
}

pub fn like() -> &'static str {
    "Like"
}

pub fn liked_description() -> &'static str {
    "Songs you've liked"
}

pub fn loading_liked() -> &'static str {
    "Loading liked songs…"
}

pub fn liked_empty() -> &'static str {
    "No liked songs yet"
}

pub fn playlist() -> &'static str {
    "Playlist"
}

pub fn playlists() -> &'static str {
    "Playlists"
}

pub fn search() -> &'static str {
    "Search"
}

pub fn sign_in() -> &'static str {
    "Sign In"
}

pub fn signing_in() -> &'static str {
    "Signing in…"
}

pub fn sign_in_subtitle() -> &'static str {
    "Sign in to your private music library"
}

pub fn username() -> &'static str {
    "Username"
}

pub fn password() -> &'static str {
    "Password"
}

pub fn loading_songs() -> &'static str {
    "Loading songs…"
}

pub fn loading_playlist() -> &'static str {
    "Loading playlist…"
}

pub fn loading_playlists() -> &'static str {
    "Loading playlists…"
}

pub fn loading() -> &'static str {
    "Loading…"
}

pub fn unable_to_start_playback() -> &'static str {
    "Unable to start playback"
}

pub fn welcome_back() -> &'static str {
    "Welcome back"
}

pub fn your_library() -> &'static str {
    "Your private library"
}

pub fn playing_next() -> &'static str {
    "Playing Next"
}

pub fn playing_from() -> &'static str {
    "Playing from"
}

pub fn now_playing() -> &'static str {
    "Now Playing"
}

pub fn clear() -> &'static str {
    "Clear"
}

pub fn queue_empty() -> &'static str {
    "Your queue is empty"
}

pub fn play_next() -> &'static str {
    "Play Next"
}

pub fn move_to_end() -> &'static str {
    "Move to End"
}

pub fn remove_from_queue() -> &'static str {
    "Remove"
}

pub fn add_to_queue() -> &'static str {
    "Add to Queue"
}

pub fn listen_now() -> &'static str {
    "Listen Now"
}

pub fn see_all() -> &'static str {
    "See All"
}

pub fn home_empty_title() -> &'static str {
    "Your library is waiting"
}

pub fn home_empty_body() -> &'static str {
    "Upload music to start listening. New songs will show up here."
}

pub fn add_music() -> &'static str {
    "Add Music"
}

pub fn choose_music_or_lyrics_files() -> &'static str {
    "Choose Music or LRC Files"
}

pub fn songs_in_library(count: i64) -> String {
    if count == 1 {
        "1 song in your library".to_owned()
    } else {
        format!("{count} songs in your library")
    }
}
