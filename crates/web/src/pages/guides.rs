//! The guides: the list every guide page, the index and the sitemap read, and their handlers.

use askama::Template;
use axum::extract::Path;
use axum::extract::rejection::PathRejection;
use axum::http::StatusCode;
use axum::http::header::LOCATION;
use axum::response::{IntoResponse, Response};

use super::jsonld::{Article, BreadcrumbList, script_json};
use super::{Frame, SPECULATION_RULES, errors, page};
use crate::components::QueryInputView;

const CACHE: &str = "public, max-age=3600";

/// One guide: its slug under `/guide/`, head metadata, index category and related guides.
pub struct Guide {
    pub slug: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    /// File name under `/static/og/`.
    pub og_image: &'static str,
    /// Slugs.
    pub related: &'static [&'static str],
}

/// Every guide, in the order and categories of the V1 index.
pub const GUIDES: &[Guide] = &[
    Guide {
        slug: "email-auth",
        title: "Email Authentication Guide: SPF, DKIM, and DMARC Explained",
        description: "Learn how to check SPF, DKIM, and DMARC records for your domain. Understand email authentication, why it matters, common mistakes, and how to debug delivery issues.",
        category: "Email Security",
        og_image: "email.png",
        related: &[
            "record-types",
            "dane-tlsa",
            "dkim",
            "bimi",
            "certificate-chain",
            "ip-enrichment",
        ],
    },
    Guide {
        slug: "email-deliverability",
        title: "How to Diagnose Email Deliverability Problems",
        description: "Step-by-step guide to diagnosing why emails land in spam or get rejected. Covers SPF failures, DMARC policy, DKIM misalignment, blacklists, and how to check each.",
        category: "Email Security",
        og_image: "email.png",
        related: &[
            "email-auth",
            "mta-sts",
            "record-types",
            "dnsbl",
            "tls-rpt",
            "dane-tlsa",
        ],
    },
    Guide {
        slug: "mta-sts",
        title: "MTA-STS: Enforcing Encrypted Email Transport",
        description: "How MTA-STS prevents downgrade attacks on email transport, how to publish the policy file and DNS record, and how to verify your setup.",
        category: "Email Security",
        og_image: "email.png",
        related: &[
            "email-auth",
            "email-deliverability",
            "tls-rpt",
            "dane-tlsa",
            "record-types",
        ],
    },
    Guide {
        slug: "dkim",
        title: "DKIM Deep Dive: Keys, Selectors, and Rotation",
        description: "Understand how DKIM signing works, choose between RSA and Ed25519 keys, manage selectors, and rotate keys safely without breaking email delivery.",
        category: "Email Security",
        og_image: "email.png",
        related: &["email-auth", "bimi", "dnsbl"],
    },
    Guide {
        slug: "bimi",
        title: "BIMI: Brand Indicators for Message Identification",
        description: "How to set up BIMI to display your brand logo in email clients. Covers DNS records, SVG Tiny PS requirements, VMC certificates, and validation with netray's email checks.",
        category: "Email Security",
        og_image: "email.png",
        related: &["dkim", "email-auth", "mta-sts"],
    },
    Guide {
        slug: "tls-rpt",
        title: "TLS-RPT: SMTP TLS Reporting",
        description: "How to deploy TLS-RPT (RFC 8460) to receive reports about SMTP TLS failures. Covers DNS record format, report structure, and integration with MTA-STS and DANE.",
        category: "Email Security",
        og_image: "email.png",
        related: &["mta-sts", "dane-tlsa", "email-auth"],
    },
    Guide {
        slug: "dnsbl",
        title: "DNS Blocklists (DNSBL): How They Work",
        description: "How DNS-based blocklists work, which major blocklists matter, how to check IP reputation manually, and how to delist. Includes practical dig examples and netray's email checks.",
        category: "Email Security",
        og_image: "email.png",
        related: &["email-auth", "email-deliverability", "dkim"],
    },
    Guide {
        slug: "certificate-chain",
        title: "TLS Certificate Chain Explained: How to Check SSL Certificate Chains",
        description: "Understand how TLS certificate chains work, from leaf to root. Learn how to check SSL certificate chains, diagnose chain issues, and avoid common mistakes.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &["dane-tlsa", "email-auth", "record-types", "ip-enrichment"],
    },
    Guide {
        slug: "hsts",
        title: "HSTS Preloading: A Practical Guide",
        description: "How HTTP Strict Transport Security works, how to configure max-age and includeSubDomains correctly, and how to get your domain onto the browser preload list.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &[
            "certificate-chain",
            "security-headers",
            "http-redirects",
            "certificate-transparency",
            "dane-tlsa",
            "domain-security-audit",
        ],
    },
    Guide {
        slug: "certificate-transparency",
        title: "Understanding Certificate Transparency",
        description: "How Certificate Transparency logs work, what SCTs are, how browsers enforce CT, and how to check whether your certificate is properly logged.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &["certificate-chain", "caa-records", "hsts", "dane-tlsa"],
    },
    Guide {
        slug: "letsencrypt-vs-commercial",
        title: "Let's Encrypt vs Commercial CAs: Which Should You Use?",
        description: "A practical comparison of Let's Encrypt and paid certificate authorities — coverage, features, automation, use cases, and when a commercial CA is worth the cost.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &[
            "certificate-chain",
            "certificate-transparency",
            "hsts",
            "caa-records",
        ],
    },
    Guide {
        slug: "multi-ip-tls",
        title: "Understanding Multi-IP TLS Consistency",
        description: "Why domains hosted across multiple IPs or CDN edge nodes can serve different TLS configurations, how to detect inconsistencies, and why they matter.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &[
            "certificate-chain",
            "certificate-transparency",
            "hsts",
            "domain-security-audit",
        ],
    },
    Guide {
        slug: "tls-protocol",
        title: "TLS Protocol and Cipher Suite Guide: Version, Forward Secrecy, AEAD",
        description: "Understand TLS version negotiation, forward secrecy, and AEAD cipher suites. Learn why TLS 1.3 matters, what makes a cipher suite secure, and how to check your server's configuration.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &[
            "certificate-chain",
            "certificate-management",
            "encrypted-client-hello",
            "multi-ip-tls",
            "hsts",
        ],
    },
    Guide {
        slug: "encrypted-client-hello",
        title: "Encrypted Client Hello (ECH): How It Works and How to Enable It",
        description: "Understand Encrypted Client Hello (ECH), the TLS extension that hides the hostname from network observers. Learn how ECH works with HTTPS DNS records, how to enable it, and how to verify it.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &["tls-protocol", "record-types", "hsts", "certificate-chain"],
    },
    Guide {
        slug: "certificate-management",
        title: "Certificate Management: Expiry, Lifetime, SAN Quality, and AIA",
        description: "Practical guide to TLS certificate management: monitoring expiry windows, understanding lifetime limits, validating SAN lists, and checking AIA reachability for robust HTTPS deployments.",
        category: "TLS & HTTPS",
        og_image: "tls.png",
        related: &[
            "certificate-chain",
            "tls-protocol",
            "certificate-transparency",
            "letsencrypt-vs-commercial",
        ],
    },
    Guide {
        slug: "security-headers",
        title: "Security Headers Overview",
        description: "A guide to essential HTTP security headers: X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy, and practical server configuration examples.",
        category: "HTTP Security",
        og_image: "http.png",
        related: &["hsts", "csp", "cors", "http-redirects"],
    },
    Guide {
        slug: "csp",
        title: "Content Security Policy (CSP): A Complete Guide",
        description: "A comprehensive guide to Content Security Policy: directives, common mistakes, report-only mode, and practical examples for tightening your CSP.",
        category: "HTTP Security",
        og_image: "http.png",
        related: &["security-headers", "cors", "hsts", "cookie-security"],
    },
    Guide {
        slug: "cors",
        title: "Cross-Origin Resource Sharing (CORS) Explained",
        description: "How CORS works, preflight requests, credentialed requests, wildcard pitfalls, and common misconfigurations with practical debugging examples.",
        category: "HTTP Security",
        og_image: "http.png",
        related: &["csp", "security-headers", "cookie-security", "hsts"],
    },
    Guide {
        slug: "cookie-security",
        title: "Cookie Security: Flags, Prefixes, and Best Practices",
        description: "How to secure cookies with Secure, HttpOnly, SameSite flags, __Host- and __Secure- prefixes, and a practical session-cookie hardening checklist.",
        category: "HTTP Security",
        og_image: "http.png",
        related: &["security-headers", "cors", "csp", "hsts"],
    },
    Guide {
        slug: "http-redirects",
        title: "HTTP Redirects and HTTPS Upgrade",
        description: "HTTP redirect status codes (301, 302, 307, 308), redirect chain performance and SEO impact, HSTS interaction, and practical server configuration examples.",
        category: "HTTP Security",
        og_image: "http.png",
        related: &["hsts", "security-headers", "dane-tlsa"],
    },
    Guide {
        slug: "record-types",
        title: "DNS Record Types Explained: A Complete Reference",
        description: "A complete reference of DNS record types: A, AAAA, CNAME, MX, TXT, NS, SOA, SRV, CAA, TLSA, and more. Learn what each record does, when to use it, and common pitfalls.",
        category: "DNS",
        og_image: "dns.png",
        related: &[
            "email-auth",
            "dane-tlsa",
            "certificate-chain",
            "ip-enrichment",
        ],
    },
    Guide {
        slug: "dnssec",
        title: "DNSSEC: How to Enable and Verify It",
        description: "How DNSSEC works, how to enable it at your registrar, how to verify the chain of trust, and what to do when validation fails.",
        category: "DNS",
        og_image: "dns.png",
        related: &[
            "record-types",
            "dane-tlsa",
            "caa-records",
            "lame-delegation",
        ],
    },
    Guide {
        slug: "dns-propagation",
        title: "How to Check DNS Propagation",
        description: "What DNS propagation means, why it takes time, how to check whether your record changes have reached resolvers worldwide, and how to speed things up.",
        category: "DNS",
        og_image: "dns.png",
        related: &["record-types", "dnssec", "lame-delegation", "email-auth"],
    },
    Guide {
        slug: "caa-records",
        title: "CAA Records: Locking Down Certificate Issuance",
        description: "How Certification Authority Authorization DNS records work, how to set them up to restrict which CAs can issue certificates for your domain, and how to check them.",
        category: "DNS",
        og_image: "dns.png",
        related: &[
            "certificate-chain",
            "dane-tlsa",
            "dnssec",
            "certificate-transparency",
        ],
    },
    Guide {
        slug: "lame-delegation",
        title: "DNS Lame Delegation: What It Is and How to Fix It",
        description: "What lame delegation means in DNS, why it causes resolution failures, how to detect it, and how to fix it at the registrar and DNS provider.",
        category: "DNS",
        og_image: "dns.png",
        related: &[
            "record-types",
            "dnssec",
            "dns-propagation",
            "domain-security-audit",
        ],
    },
    Guide {
        slug: "ip-enrichment",
        title: "What Your IP Address Reveals: IP Enrichment Explained",
        description: "Discover what your IP address reveals about you: geolocation, ISP, VPN detection, cloud provider, threat intelligence, and more. Learn how IP enrichment works.",
        category: "IP & Reputation",
        og_image: "ip.png",
        related: &[
            "record-types",
            "email-auth",
            "certificate-chain",
            "dane-tlsa",
        ],
    },
    Guide {
        slug: "asn",
        title: "What Is an ASN and Why Does It Matter?",
        description: "What Autonomous System Numbers are, how the internet's routing works, how to look up an ASN, and why ASN data is used for geolocation, reputation, and access control.",
        category: "IP & Reputation",
        og_image: "ip.png",
        related: &[
            "ip-enrichment",
            "vpn-leak",
            "domain-security-audit",
            "domain-investigation",
        ],
    },
    Guide {
        slug: "vpn-leak",
        title: "VPN Leak Detection: What Your IP Reveals",
        description: "How to detect VPN and DNS leaks, what information a VPN should and shouldn't hide, and how to verify your VPN is actually protecting your traffic.",
        category: "IP & Reputation",
        og_image: "ip.png",
        related: &[
            "ip-enrichment",
            "asn",
            "domain-investigation",
            "domain-security-audit",
        ],
    },
    Guide {
        slug: "scoring",
        title: "How netray Scores a Domain",
        description: "How netray produces an A+–F domain health grade: scoring algorithm, section weights, email security for sending vs. receiving domains, and the dkim_selectors parameter.",
        category: "Domain Audits",
        og_image: "landing.png",
        related: &["email-auth", "mta-sts", "domain-security-audit"],
    },
    Guide {
        slug: "domain-security-audit",
        title: "Domain Security Audit: Automated with netray or step-by-step",
        description: "Start with netray for an A+–F domain health grade in one shot, or follow the step-by-step audit across DNS, TLS, email authentication, and IP reputation.",
        category: "Domain Audits",
        og_image: "landing.png",
        related: &[
            "security-headers",
            "email-auth",
            "certificate-chain",
            "dane-tlsa",
            "domain-investigation",
        ],
    },
    Guide {
        slug: "dane-tlsa",
        title: "DANE/TLSA Setup Guide: DNS-Based Certificate Authentication",
        description: "Learn how to configure DANE and TLSA records for DNS-based certificate authentication. Understand certificate usage, selector, and matching type fields, and avoid common setup mistakes.",
        category: "Domain Audits",
        og_image: "dns.png",
        related: &[
            "certificate-chain",
            "record-types",
            "tls-rpt",
            "email-auth",
            "ip-enrichment",
        ],
    },
    Guide {
        slug: "domain-investigation",
        title: "A Suspicious Domain Investigation Checklist",
        description: "How to systematically investigate a suspicious domain: DNS history, TLS certificate details, IP reputation, registration patterns, and what each finding means.",
        category: "Domain Audits",
        og_image: "landing.png",
        related: &[
            "ip-enrichment",
            "asn",
            "email-auth",
            "domain-security-audit",
        ],
    },
];

/// The guides whose page also carries a breadcrumb trail.
const BREADCRUMBS: [&str; 9] = [
    "dkim",
    "bimi",
    "tls-rpt",
    "dnsbl",
    "security-headers",
    "csp",
    "cors",
    "cookie-security",
    "http-redirects",
];

/// The guide with `slug`.
pub fn find(slug: &str) -> Option<&'static Guide> {
    GUIDES.iter().find(|g| g.slug == slug)
}

/// The query input of a guide's tool entry.
pub(crate) fn tool(action: &str, name: &str, placeholder: &str, submit: &str) -> QueryInputView {
    let label = match name {
        "d" | "domain" => "Domain".to_string(),
        "h" => "Host".to_string(),
        "url" => "URL".to_string(),
        "q" => "Query".to_string(),
        "ip" => "IP address".to_string(),
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    };
    QueryInputView {
        action: action.to_string(),
        name: name.to_string(),
        label,
        placeholder: placeholder.to_string(),
        value: None,
        submit: submit.to_string(),
    }
}

macro_rules! guide_templates {
    ($($name:ident => $slug:tt, $path:tt;)*) => {
        $(
            #[derive(Template)]
            #[template(path = $path)]
            struct $name {
                frame: Frame,
                guide: &'static Guide,
                related: Vec<&'static Guide>,
                ld: Vec<String>,
            }
        )*

        fn render(
            frame: Frame,
            guide: &'static Guide,
            related: Vec<&'static Guide>,
            ld: Vec<String>,
        ) -> Response {
            match guide.slug {
                $($slug => page(StatusCode::OK, CACHE, &$name { frame, guide, related, ld }),)*
                _ => super::server_error(),
            }
        }
    };
}

guide_templates! {
    EmailAuthGuide => "email-auth", "pages/guides/email-auth.html";
    EmailDeliverabilityGuide => "email-deliverability", "pages/guides/email-deliverability.html";
    MtaStsGuide => "mta-sts", "pages/guides/mta-sts.html";
    DkimGuide => "dkim", "pages/guides/dkim.html";
    BimiGuide => "bimi", "pages/guides/bimi.html";
    TlsRptGuide => "tls-rpt", "pages/guides/tls-rpt.html";
    DnsblGuide => "dnsbl", "pages/guides/dnsbl.html";
    CertificateChainGuide => "certificate-chain", "pages/guides/certificate-chain.html";
    HstsGuide => "hsts", "pages/guides/hsts.html";
    CertificateTransparencyGuide => "certificate-transparency", "pages/guides/certificate-transparency.html";
    LetsencryptVsCommercialGuide => "letsencrypt-vs-commercial", "pages/guides/letsencrypt-vs-commercial.html";
    MultiIpTlsGuide => "multi-ip-tls", "pages/guides/multi-ip-tls.html";
    TlsProtocolGuide => "tls-protocol", "pages/guides/tls-protocol.html";
    EncryptedClientHelloGuide => "encrypted-client-hello", "pages/guides/encrypted-client-hello.html";
    CertificateManagementGuide => "certificate-management", "pages/guides/certificate-management.html";
    SecurityHeadersGuide => "security-headers", "pages/guides/security-headers.html";
    CspGuide => "csp", "pages/guides/csp.html";
    CorsGuide => "cors", "pages/guides/cors.html";
    CookieSecurityGuide => "cookie-security", "pages/guides/cookie-security.html";
    HttpRedirectsGuide => "http-redirects", "pages/guides/http-redirects.html";
    RecordTypesGuide => "record-types", "pages/guides/record-types.html";
    DnssecGuide => "dnssec", "pages/guides/dnssec.html";
    DnsPropagationGuide => "dns-propagation", "pages/guides/dns-propagation.html";
    CaaRecordsGuide => "caa-records", "pages/guides/caa-records.html";
    LameDelegationGuide => "lame-delegation", "pages/guides/lame-delegation.html";
    IpEnrichmentGuide => "ip-enrichment", "pages/guides/ip-enrichment.html";
    AsnGuide => "asn", "pages/guides/asn.html";
    VpnLeakGuide => "vpn-leak", "pages/guides/vpn-leak.html";
    ScoringGuide => "scoring", "pages/guides/scoring.html";
    DomainSecurityAuditGuide => "domain-security-audit", "pages/guides/domain-security-audit.html";
    DaneTlsaGuide => "dane-tlsa", "pages/guides/dane-tlsa.html";
    DomainInvestigationGuide => "domain-investigation", "pages/guides/domain-investigation.html";
}

/// `GET /guide/{slug}`.
pub async fn show(slug: Result<Path<String>, PathRejection>) -> Response {
    let Some(guide) = slug.ok().and_then(|Path(slug)| find(&slug)) else {
        return errors::not_found().await;
    };
    let url = format!("https://netray.info/guide/{}", guide.slug);
    let mut ld = vec![script_json(&Article::new(
        guide.title,
        guide.description,
        &url,
    ))];
    if BREADCRUMBS.contains(&guide.slug) {
        ld.push(script_json(&BreadcrumbList::new(&[
            ("Home", Some("https://netray.info/")),
            ("Guides", Some("https://netray.info/guide/")),
            (guide.title, None),
        ])));
    }
    let frame = Frame {
        title: format!("{} - netray", guide.title),
        description: guide.description.to_string(),
        canonical: format!("/guide/{}", guide.slug),
        og_image: format!("https://netray.info/static/og/{}", guide.og_image),
        rules: SPECULATION_RULES,
    };
    let related = guide.related.iter().filter_map(|s| find(s)).collect();
    render(frame, guide, related, ld)
}

#[derive(Template)]
#[template(path = "pages/guide_index.html")]
struct GuideIndex {
    frame: Frame,
    categories: Vec<(&'static str, Vec<&'static Guide>)>,
    ld: Vec<String>,
}

/// `GET /guide/`.
pub async fn index() -> Response {
    let mut categories: Vec<(&'static str, Vec<&'static Guide>)> = Vec::new();
    for guide in GUIDES {
        match categories.last_mut() {
            Some((category, guides)) if *category == guide.category => guides.push(guide),
            _ => categories.push((guide.category, vec![guide])),
        }
    }
    let ld = vec![script_json(&BreadcrumbList::new(&[
        ("netray.info", Some("https://netray.info")),
        ("Guides", Some("https://netray.info/guide/")),
    ]))];
    let frame = Frame::new(
        "All Guides - netray",
        "Reference guides, how-tos, and deep dives on DNS, TLS, IP, and email security for DevOps engineers and SREs.",
        "/guide/",
    );
    page(
        StatusCode::OK,
        CACHE,
        &GuideIndex {
            frame,
            categories,
            ld,
        },
    )
}

/// `GET /guide`: a 301 to the index.
pub async fn index_redirect() -> Response {
    (StatusCode::MOVED_PERMANENTLY, [(LOCATION, "/guide/")]).into_response()
}
