use actix_web::{web, HttpRequest, HttpResponse};

use crate::bencode;
use crate::bittorrent::{AnnounceRequest, AnnounceResponse, Peer, ScrapeRequest};
use crate::storage::{PeerStorage, PeerStore};

// This will eventually be read from the configuration YAML.
const INTERVAL: u32 = 60;

pub fn parse_announce(data: web::Data<PeerStore>, req: HttpRequest) -> HttpResponse {
    let announce_request = AnnounceRequest::new(req.query_string(), req.connection_info().remote());

    match announce_request {
        Ok(parsed_req) => {
            let peer_list = data.get_peers(parsed_req.info_hash, parsed_req.numwant.unwrap());
            let mut peers = Vec::new();
            let mut peers6 = Vec::new();

            for peer in peer_list {
                match peer {
                    Peer::V4(p) => peers.push(p),
                    Peer::V6(p) => peers6.push(p),
                }
            }

            peers.sort();
            peers6.sort();

            // Dummy values, the actuals will come from the torrent storage
            let response = AnnounceResponse::new(INTERVAL, 100, 23, peers, peers6);
            let bencoded = bencode::encode_announce_response(response.unwrap());
            HttpResponse::Ok().content_type("text/plain").body(bencoded)
        }

        // If the request is not parse-able, short-circuit and respond with failure
        Err(failure) => {
            let bencoded = bencode::encode_announce_response(failure);
            HttpResponse::Ok().content_type("text/plain").body(bencoded)
        }
    }
}

pub fn parse_scrape(data: web::Data<PeerStore>, req: HttpRequest) {
    let scrape_request = ScrapeRequest::new(req.query_string());

    match scrape_request {
        Ok(parsed_req) => {
            // Need to get the information for torrents and all
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::dev::Service;
    use actix_web::{guard, test, web, App, HttpResponse};

    use crate::bittorrent::{Peerv4, Peerv6};
    use crate::storage::{PeerStorage, PeerStore};
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn index_get_not_allowed() {
        let data = web::Data::new(PeerStore::new().unwrap());
        let mut app = test::init_service(
            App::new()
                .register_data(data.clone())
                .service(
                    web::resource("announce")
                        .guard(guard::Header("content-type", "text/plain"))
                        .route(web::get().to(parse_announce)),
                )
                .service(
                    web::resource("scrape")
                        .guard(guard::Header("content-type", "text/plain"))
                        .route(web::get().to(parse_scrape)),
                )
                .default_service(web::route().to(HttpResponse::MethodNotAllowed)),
        );
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::block_on(app.call(req)).unwrap();

        assert!(resp.status().is_client_error());
    }

    #[test]
    fn announce_get_malformed() {
        let data = web::Data::new(PeerStore::new().unwrap());
        let app = test::init_service(
            App::new()
                .register_data(data.clone())
                .service(
                    web::resource("announce")
                        .guard(guard::Header("content-type", "text/plain"))
                        .route(web::get().to(parse_announce)),
                )
                .service(
                    web::resource("scrape")
                        .guard(guard::Header("content-type", "text/plain"))
                        .route(web::get().to(parse_scrape)),
                )
                .default_service(web::route().to(HttpResponse::MethodNotAllowed)),
        );

        let proper_resp = HttpResponse::Ok()
            .content_type("text/plain")
            .body("d14:failure_reason17:Malformed requeste".as_bytes());
        let req = test::TestRequest::get()
            .uri("/announce?bad_stuff=123")
            .to_http_request();
        let resp = test::block_on(parse_announce(data, req)).unwrap();

        assert_eq!(
            resp.body().as_ref().unwrap(),
            proper_resp.body().as_ref().unwrap()
        );
    }

    #[test]
    fn announce_get_success() {
        let store = PeerStore::new().unwrap();
        let info_hash = "A1B2C3D4E5F6G7H8I9J0".to_string();
        let peerv4_1 = Peer::V4(Peerv4 {
            peer_id: "ABCDEFGHIJKLMNOPQRST".to_string(),
            ip: Ipv4Addr::LOCALHOST,
            port: 6893,
        });

        let peerv4_2 = Peer::V4(Peerv4 {
            peer_id: "ABCDEFGHIJKLMNOPQRST".to_string(),
            ip: Ipv4Addr::BROADCAST,
            port: 6894,
        });

        store.put_seeder(info_hash.clone(), peerv4_2.clone());
        store.put_seeder(info_hash.clone(), peerv4_1.clone());

        let peerv6_1 = Peer::V6(Peerv6 {
            peer_id: "ABCDEFGHIJKLMNOPABCD".to_string(),
            ip: Ipv6Addr::new(
                0x2001, 0x0db8, 0x85a3, 0x0000, 0x0000, 0x8a2e, 0x0370, 0x7334,
            ),
            port: 6681,
        });

        store.put_seeder(info_hash.clone(), peerv6_1.clone());

        let peerv6_2 = Peer::V6(Peerv6 {
            peer_id: "ABCDEFGHIJKLMNOPZZZZ".to_string(),
            ip: Ipv6Addr::new(
                0xfe80, 0x0000, 0x0000, 0x0000, 0x0202, 0xb3ff, 0xfe1e, 0x8329,
            ),
            port: 6699,
        });

        store.put_seeder(info_hash.clone(), peerv6_2.clone());

        let data = web::Data::new(store);
        let app = test::init_service(
            App::new()
                .register_data(data.clone())
                .service(
                    web::resource("announce")
                        .guard(guard::Header("content-type", "text/plain"))
                        .route(web::get().to(parse_announce)),
                )
                .service(
                    web::resource("scrape")
                        .guard(guard::Header("content-type", "text/plain"))
                        .route(web::get().to(parse_scrape)),
                )
                .default_service(web::route().to(HttpResponse::MethodNotAllowed)),
        );

        let uri = "/announce?info_hash=A1B2C3D4E5F6G7H8I9J0\
                   &peer_id=ABCDEFGHIJKLMNOPQRST&port=6881&uploaded=0&downloaded=0\
                   &left=727955456&event=started&numwant=100&no_peer_id=1&compact=1";

        let proper_resp = HttpResponse::Ok().content_type("text/plain").body("d8:completei100e10:incompletei23e8:intervali60e5:peersli127ei0ei0ei1ei26ei237ei255ei255ei255ei255ei26ei238ee6:peers6li32ei1ei13ei184ei133ei163ei0ei0ei0ei0ei138ei46ei3ei112ei115ei52ei26ei25ei254ei128ei0ei0ei0ei0ei0ei0ei2ei2ei179ei255ei254ei30ei131ei41ei26ei43ee10:tracker_id0:e".as_bytes());
        let req = test::TestRequest::get().uri(uri).to_http_request();
        let resp = test::block_on(parse_announce(data, req)).unwrap();

        assert_eq!(
            resp.body().as_ref().unwrap(),
            proper_resp.body().as_ref().unwrap()
        );
    }
}
